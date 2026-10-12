//! AI-texturing provider interface — the v0.1 design hook (P2 research
//! track, requirements §10 "AI-assisted texturing"; research 04 §13).
//!
//! A *provider* is anything that can regenerate texels of an RGBA8 tile:
//! a local `candle`/ONNX model, a sidecar (ComfyUI) behind a local
//! socket, or — opt-in only — a remote API. This module is the contract
//! between such a provider and the app; it ships **no model and no
//! host binding**. Two generation kinds:
//!
//! - [`GenerationKind::Inpaint`]: regenerate the texels under a region
//!   mask (the "generative brush" on the 2D tile).
//! - [`GenerationKind::SeamlessTile`]: produce a tileable variant of the
//!   source (optionally restricted to a mask, e.g. user-drawn edge
//!   constraints in the Tiled-Diffusion style).
//!
//! ```ignore
//! use umber_plugin_sdk::provider::*;
//! use umber_plugin_sdk::ImageMut;
//!
//! fn gen(req: &GenerationRequest<'_>, out: &mut ImageMut<'_>) -> Result<(), ProviderError> {
//!     out.data.copy_from_slice(req.source.data); // a real model goes here
//!     Ok(())
//! }
//!
//! static MINE: FnProvider = FnProvider::new(
//!     ProviderInfo::new("my-sd", "sd15-inpaint", "1", Locality::InProcess,
//!                       Capabilities::INPAINT, true),
//!     gen,
//! );
//!
//! let mut reg: ProviderRegistry<'_, 4> = ProviderRegistry::new();
//! reg.register(&MINE)?;
//! let result = reg.generate(&GenerationRequest::inpaint(src, mask).with_seed(7), &mut out)?;
//! ```
//!
//! # The WASM-safe subset
//!
//! The SDK's constraint is [`crate::umber_node!`]'s pattern: `no_std`,
//! **no allocator**, a compile-time-validated `static` record plus a plain
//! `fn` path. Everything here keeps to it:
//!
//! - every type is borrowed or `Copy`; ids and model names are
//!   `&'static str`; the output is written into a caller-pre-sized
//!   [`ImageMut`] exactly like an [`crate::EvalFn`] (no owned bytes ever
//!   cross the interface);
//! - [`ProviderInfo::new`] is a `const fn` whose checks are compile-time
//!   panics when used in a `static` (a bad provider id never builds, as
//!   with `node_def_record`);
//! - [`FnProvider`] is the `static`-declarable provider (info + fn
//!   pointer) a wasm guest writes; [`TextureProvider`] is the trait the
//!   host-side registry consumes, so host-native providers (a candle
//!   model holding GPU state) implement it directly;
//! - [`ProviderRegistry`] is fixed-capacity (`const N`), holding
//!   `&dyn TextureProvider` — no `Box`, no `Vec`.
//!
//! No `umber_provider!` export macro yet: the v1 wire (WIRE.md /
//! `wire_consts`) has no provider ABI — one image input, ≤ 4 params, no
//! mask, no u64 seed — and the wire constants live byte-identically in
//! umber-wasm. Emitting C-ABI exports no host binds would be dead ABI;
//! the export macro lands with the umber-wasm side, when the request
//! layout below gets a region encoding in the shared constants file.
//!
//! # Judgment calls
//!
//! - **Local-first is a registry policy, not a type restriction.** The
//!   research decision is a model-agnostic interface (local candle / ONNX
//!   / remote API), so [`Locality::Remote`] exists; but a request refuses
//!   remote providers unless [`GenerationRequest::allow_remote`] is set
//!   (default `false`), and auto-resolution always prefers a local
//!   provider over a remote one, regardless of registration order.
//! - **Provenance is built by the registry, not reported by the
//!   provider.** [`Provenance`] is assembled from the validated request
//!   (kind, seed, strength, dimensions) and FNV-1a 64 digests of
//!   prompt/source/mask/output, which the registry computes itself — a
//!   provider cannot misreport those. The id/model/locality fields come
//!   from [`TextureProvider::info`], read at register *and* at every
//!   generate: they are only as honest as the provider's contract that
//!   `info()` is stable (an [`FnProvider`]'s is, by construction; a
//!   native impl must keep it so — v1 does not snapshot it). FNV-1a
//!   because it is dependency-free, `no_std` and stable across
//!   platforms — a fingerprint for the journal, not a cryptographic hash.
//! - **Mask encoding:** one byte per texel, `len == width*height`, same
//!   row order as the image. `0` = keep, nonzero = may regenerate;
//!   intermediate values are a feather weight whose interpretation is the
//!   provider's. An all-zero mask is rejected ([`ProviderError::EmptyMask`])
//!   rather than treated as a no-op, so a stale/empty selection surfaces.
//! - **Strength** is in `[0, 1]` (img2img denoise semantics: `1.0`
//!   regenerates outright, lower keeps more of the source).
//!   [`DEFAULT_STRENGTH`] is `1.0` — inpainting's usual setting.
//! - **Prompt** is carried as an opaque UTF-8 string (≤
//!   [`MAX_PROMPT_LEN`] bytes, empty = unconditioned). Not in the brief's
//!   minimum, but an inpaint interface without any conditioning would be
//!   rewritten at first use; its meaning is entirely the provider's.
//! - **Provider ids** follow node-name rules (`[A-Za-z0-9_-]`, 1..=64
//!   bytes) so they can later travel in the same wire records.
//!
//! # v1 boundary — what this interface deliberately does NOT promise
//!
//! - No models, no inference, no host binding (umber-wasm and umber-app
//!   don't consume this yet; delivery is post-0.1 per the requirements
//!   row).
//! - No async, progress reporting or cancellation: `generate` is a
//!   blocking call. A real provider (minutes per asset) needs a job API;
//!   that is a v2 shape, not bolted onto this one.
//! - No UV-seam awareness: inpainting is 2D on the tile. Seam-margin
//!   blending and dilation are the caller's job (the paint pipeline's).
//! - No reproducibility across providers, hardware or model versions for
//!   the same seed. [`ProviderInfo::deterministic`] is the provider's
//!   own claim about *its* reruns, recorded in provenance, never checked.
//! - No conditioning beyond the prompt (no ControlNet depth/normal
//!   inputs, no reference images, no multi-view): one source image in.
//! - RGBA8 only, output size == input size (the `EvalFn` contract).
//! - No licensing, content policy or network enforcement beyond the
//!   `allow_remote` gate; a provider that claims `InProcess` and phones
//!   home is out of this interface's reach.

use crate::wire_consts::MAX_NAME_LEN;
use crate::{Image, ImageMut};

/// Default request strength: regenerate the masked texels outright.
pub const DEFAULT_STRENGTH: f32 = 1.0;

/// Longest accepted prompt, in UTF-8 bytes.
pub const MAX_PROMPT_LEN: usize = 1024;

/// What a request asks the provider to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationKind {
    /// Regenerate the texels under a region mask.
    Inpaint,
    /// Produce a seamlessly tiling variant of the source.
    SeamlessTile,
}

/// The set of [`GenerationKind`]s a provider supports (a bitset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities(u32);

impl Capabilities {
    /// No capabilities.
    pub const NONE: Self = Self(0);
    /// [`GenerationKind::Inpaint`].
    pub const INPAINT: Self = Self(1);
    /// [`GenerationKind::SeamlessTile`].
    pub const SEAMLESS_TILE: Self = Self(1 << 1);

    /// The union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The bit for one kind.
    #[must_use]
    pub const fn of(kind: GenerationKind) -> Self {
        match kind {
            GenerationKind::Inpaint => Self::INPAINT,
            GenerationKind::SeamlessTile => Self::SEAMLESS_TILE,
        }
    }

    /// True when `kind` is in the set.
    #[must_use]
    pub const fn supports(self, kind: GenerationKind) -> bool {
        self.0 & Self::of(kind).0 != 0
    }

    /// True for the empty set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Where a provider's compute runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locality {
    /// Inside the Umber process (a candle/ONNX model, a wasm guest).
    InProcess,
    /// A separate process on the same machine (e.g. a ComfyUI sidecar).
    LocalSidecar,
    /// Off-machine. Only used when a request sets `allow_remote`.
    Remote,
}

impl Locality {
    /// True for `InProcess` and `LocalSidecar`.
    #[must_use]
    pub const fn is_local(self) -> bool {
        !matches!(self, Self::Remote)
    }
}

/// Why registration or generation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderError {
    /// A provider with the same id is already registered.
    DuplicateId,
    /// The registry's fixed capacity is used up.
    RegistryFull,
    /// No registered provider has the requested id.
    UnknownProvider,
    /// The named provider doesn't support the request's kind.
    Unsupported,
    /// No registered provider supports the request's kind at all.
    NoEligibleProvider,
    /// Only remote providers support the kind (or the named provider is
    /// remote) and the request didn't set `allow_remote`.
    RemoteNotAllowed,
    /// The source image is empty or its data length isn't `w*h*4`.
    BadSource,
    /// The output image's size differs from the source's.
    OutputSizeMismatch,
    /// An `Inpaint` request came without a mask.
    MissingMask,
    /// The mask length isn't `width*height`.
    MaskSizeMismatch,
    /// The mask selects no texel.
    EmptyMask,
    /// Strength is NaN or outside `[0, 1]`.
    StrengthOutOfRange,
    /// The prompt exceeds [`MAX_PROMPT_LEN`].
    PromptTooLong,
    /// The provider itself failed (model error, sidecar unreachable, …).
    ProviderFailed,
}

/// Static description of a provider. Construct via [`ProviderInfo::new`]
/// (fields are private so every instance passed its checks).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderInfo {
    id: &'static str,
    model: &'static str,
    model_version: &'static str,
    locality: Locality,
    capabilities: Capabilities,
    deterministic: bool,
}

/// True when `id` is a valid provider id (node-name rules).
#[must_use]
pub const fn is_valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    if b.is_empty() || b.len() > MAX_NAME_LEN as usize {
        return false;
    }
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            return false;
        }
        i += 1;
    }
    true
}

impl ProviderInfo {
    /// A provider description. In a `static`/`const` the checks are
    /// compile-time errors; at runtime they panic.
    ///
    /// # Panics
    ///
    /// When `id` isn't `[A-Za-z0-9_-]` of 1..=64 bytes, `model` is empty,
    /// or `capabilities` is empty.
    #[must_use]
    pub const fn new(
        id: &'static str,
        model: &'static str,
        model_version: &'static str,
        locality: Locality,
        capabilities: Capabilities,
        deterministic: bool,
    ) -> Self {
        assert!(
            is_valid_id(id),
            "provider id must be 1..=64 bytes of [A-Za-z0-9_-]"
        );
        assert!(!model.is_empty(), "provider model must be named");
        assert!(
            !capabilities.is_empty(),
            "provider must support at least one kind"
        );
        Self {
            id,
            model,
            model_version,
            locality,
            capabilities,
            deterministic,
        }
    }

    /// The registry key.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The model identifier (e.g. `"sd15-inpaint"`).
    #[must_use]
    pub const fn model(&self) -> &'static str {
        self.model
    }

    /// The model version/revision string (may be empty).
    #[must_use]
    pub const fn model_version(&self) -> &'static str {
        self.model_version
    }

    /// Where it runs.
    #[must_use]
    pub const fn locality(&self) -> Locality {
        self.locality
    }

    /// Which kinds it supports.
    #[must_use]
    pub const fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    /// The provider's own claim that the same request reruns to the same
    /// bytes on the same machine. Recorded, never verified.
    #[must_use]
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
}

/// One generation request. Build with [`GenerationRequest::inpaint`] /
/// [`GenerationRequest::seamless_tile`] and the `with_*` setters; the
/// registry validates it before any provider sees it.
#[derive(Debug, Clone, Copy)]
pub struct GenerationRequest<'a> {
    /// What to produce.
    pub kind: GenerationKind,
    /// The source tile (RGBA8, the [`Image`] contract).
    pub source: Image<'a>,
    /// One byte per texel; `0` keeps, nonzero may regenerate.
    pub mask: Option<&'a [u8]>,
    /// RNG seed handed to the provider.
    pub seed: u64,
    /// `[0, 1]`; `1.0` regenerates outright.
    pub strength: f32,
    /// Opaque conditioning text; empty = unconditioned.
    pub prompt: &'a str,
    /// Permit [`Locality::Remote`] providers. Default `false`.
    pub allow_remote: bool,
}

impl<'a> GenerationRequest<'a> {
    /// An inpaint request over `mask`, seed 0, [`DEFAULT_STRENGTH`].
    #[must_use]
    pub const fn inpaint(source: Image<'a>, mask: &'a [u8]) -> Self {
        Self {
            kind: GenerationKind::Inpaint,
            source,
            mask: Some(mask),
            seed: 0,
            strength: DEFAULT_STRENGTH,
            prompt: "",
            allow_remote: false,
        }
    }

    /// A whole-tile seamless request, seed 0, [`DEFAULT_STRENGTH`].
    #[must_use]
    pub const fn seamless_tile(source: Image<'a>) -> Self {
        Self {
            kind: GenerationKind::SeamlessTile,
            source,
            mask: None,
            seed: 0,
            strength: DEFAULT_STRENGTH,
            prompt: "",
            allow_remote: false,
        }
    }

    /// Sets the seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the strength (validated later).
    #[must_use]
    pub const fn with_strength(mut self, strength: f32) -> Self {
        self.strength = strength;
        self
    }

    /// Sets (or replaces) the region mask.
    #[must_use]
    pub const fn with_mask(mut self, mask: &'a [u8]) -> Self {
        self.mask = Some(mask);
        self
    }

    /// Sets the prompt.
    #[must_use]
    pub const fn with_prompt(mut self, prompt: &'a str) -> Self {
        self.prompt = prompt;
        self
    }

    /// Permits remote providers for this request.
    #[must_use]
    pub const fn allowing_remote(mut self) -> Self {
        self.allow_remote = true;
        self
    }

    /// Texel count of the source, `None` on overflow.
    fn texels(&self) -> Option<usize> {
        (self.source.width as usize).checked_mul(self.source.height as usize)
    }

    /// Checks the request's own invariants (provider-independent).
    ///
    /// # Errors
    ///
    /// `BadSource`, `StrengthOutOfRange`, `MissingMask`,
    /// `MaskSizeMismatch`, `EmptyMask` or `PromptTooLong`, checked in
    /// that order.
    pub fn validate(&self) -> Result<(), ProviderError> {
        let texels = self
            .texels()
            .filter(|&n| n > 0)
            .ok_or(ProviderError::BadSource)?;
        if texels.checked_mul(4) != Some(self.source.data.len()) {
            return Err(ProviderError::BadSource);
        }
        // `contains` is false for NaN, so NaN is rejected too.
        if !(0.0..=1.0).contains(&self.strength) {
            return Err(ProviderError::StrengthOutOfRange);
        }
        match (self.kind, self.mask) {
            (GenerationKind::Inpaint, None) => return Err(ProviderError::MissingMask),
            (_, Some(m)) if m.len() != texels => return Err(ProviderError::MaskSizeMismatch),
            (_, Some(m)) if m.iter().all(|&v| v == 0) => return Err(ProviderError::EmptyMask),
            _ => {}
        }
        if self.prompt.len() > MAX_PROMPT_LEN {
            return Err(ProviderError::PromptTooLong);
        }
        Ok(())
    }
}

/// FNV-1a 64 over `bytes` — the provenance fingerprint (see module docs).
#[must_use]
pub const fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < bytes.len() {
        h ^= bytes[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

/// Where a generated image came from — built by the registry from the
/// registered [`ProviderInfo`] and the validated request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Provenance {
    /// The provider's registry id.
    pub provider_id: &'static str,
    /// Its model identifier.
    pub model: &'static str,
    /// Its model version.
    pub model_version: &'static str,
    /// Where it ran.
    pub locality: Locality,
    /// The provider's determinism claim.
    pub deterministic: bool,
    /// The request kind.
    pub kind: GenerationKind,
    /// The seed used.
    pub seed: u64,
    /// The strength used.
    pub strength: f32,
    /// Output width (== source width).
    pub width: u32,
    /// Output height (== source height).
    pub height: u32,
    /// FNV-1a 64 of the prompt bytes.
    pub prompt_digest: u64,
    /// FNV-1a 64 of the source RGBA8 bytes.
    pub source_digest: u64,
    /// FNV-1a 64 of the mask bytes, `None` when no mask was sent.
    pub mask_digest: Option<u64>,
    /// FNV-1a 64 of the output RGBA8 bytes.
    pub output_digest: u64,
}

/// A finished generation: the written output image plus provenance.
#[derive(Debug, Clone, Copy)]
pub struct GenerationResult<'o> {
    /// The output, borrowed from the caller's [`ImageMut`].
    pub image: Image<'o>,
    /// Where it came from.
    pub provenance: Provenance,
}

/// A texture-generation provider. `generate` receives an
/// already-validated request and an output pre-sized to the source; it
/// must write every output byte.
pub trait TextureProvider {
    /// The provider's static description. Must return the same record
    /// for the provider's whole registration: the registry reads it at
    /// `register` (id uniqueness) and at every generate (routing and
    /// provenance).
    fn info(&self) -> &ProviderInfo;

    /// Runs one generation into `out`.
    ///
    /// # Errors
    ///
    /// Any [`ProviderError`]; [`ProviderError::ProviderFailed`] for
    /// provider-internal failures.
    fn generate(
        &self,
        request: &GenerationRequest<'_>,
        out: &mut ImageMut<'_>,
    ) -> Result<(), ProviderError>;
}

/// A provider's generate function, as [`FnProvider`] holds it (the
/// provider analogue of [`crate::EvalFn`]).
pub type GenerateFn = fn(&GenerationRequest<'_>, &mut ImageMut<'_>) -> Result<(), ProviderError>;

/// The WASM-safe provider: a `static`-declarable info record plus a
/// plain `fn` path, no state, no allocation.
#[derive(Debug, Clone, Copy)]
pub struct FnProvider {
    info: ProviderInfo,
    generate: GenerateFn,
}

impl FnProvider {
    /// Pairs an info record with its generate function.
    #[must_use]
    pub const fn new(info: ProviderInfo, generate: GenerateFn) -> Self {
        Self { info, generate }
    }
}

impl TextureProvider for FnProvider {
    fn info(&self) -> &ProviderInfo {
        &self.info
    }

    fn generate(
        &self,
        request: &GenerationRequest<'_>,
        out: &mut ImageMut<'_>,
    ) -> Result<(), ProviderError> {
        (self.generate)(request, out)
    }
}

/// A fixed-capacity set of providers, keyed by id, in registration
/// order. `N` is the capacity; no allocation.
pub struct ProviderRegistry<'p, const N: usize> {
    slots: [Option<&'p dyn TextureProvider>; N],
    len: usize,
}

impl<const N: usize> Default for ProviderRegistry<'_, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'p, const N: usize> ProviderRegistry<'p, N> {
    /// An empty registry.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [None; N],
            len: 0,
        }
    }

    /// Number of registered providers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when nothing is registered.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The fixed capacity `N`.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Registered providers, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &'p dyn TextureProvider> + '_ {
        self.slots[..self.len].iter().flatten().copied()
    }

    /// Adds a provider.
    ///
    /// # Errors
    ///
    /// [`ProviderError::DuplicateId`] when its id is taken (checked
    /// first), [`ProviderError::RegistryFull`] at capacity.
    pub fn register(&mut self, provider: &'p dyn TextureProvider) -> Result<(), ProviderError> {
        if self.get(provider.info().id()).is_some() {
            return Err(ProviderError::DuplicateId);
        }
        let slot = self
            .slots
            .get_mut(self.len)
            .ok_or(ProviderError::RegistryFull)?;
        *slot = Some(provider);
        self.len += 1;
        Ok(())
    }

    /// The provider registered under `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&'p dyn TextureProvider> {
        self.iter().find(|p| p.info().id() == id)
    }

    /// Picks a provider for `request`: the first local provider (in
    /// registration order) supporting its kind; failing that, the first
    /// remote one if the request allows remote.
    ///
    /// # Errors
    ///
    /// [`ProviderError::RemoteNotAllowed`] when only remote providers
    /// qualify and the request doesn't allow them;
    /// [`ProviderError::NoEligibleProvider`] when none support the kind.
    pub fn resolve(
        &self,
        request: &GenerationRequest<'_>,
    ) -> Result<&'p dyn TextureProvider, ProviderError> {
        let kind = request.kind;
        let supports = |p: &&'p dyn TextureProvider| p.info().capabilities().supports(kind);
        if let Some(p) = self
            .iter()
            .filter(supports)
            .find(|p| p.info().locality().is_local())
        {
            return Ok(p);
        }
        match self.iter().find(supports) {
            Some(p) if request.allow_remote => Ok(p),
            Some(_) => Err(ProviderError::RemoteNotAllowed),
            None => Err(ProviderError::NoEligibleProvider),
        }
    }

    /// Validates `request`, resolves a provider ([`Self::resolve`]) and
    /// runs it into `out`.
    ///
    /// # Errors
    ///
    /// Request validation, resolution, output-size and provider errors.
    pub fn generate<'o>(
        &self,
        request: &GenerationRequest<'_>,
        out: &'o mut ImageMut<'_>,
    ) -> Result<GenerationResult<'o>, ProviderError> {
        request.validate()?;
        let provider = self.resolve(request)?;
        run(provider, request, out)
    }

    /// Like [`Self::generate`] but with the provider named by `id`.
    ///
    /// # Errors
    ///
    /// [`ProviderError::UnknownProvider`], [`ProviderError::Unsupported`],
    /// [`ProviderError::RemoteNotAllowed`], plus [`Self::generate`]'s.
    pub fn generate_with<'o>(
        &self,
        id: &str,
        request: &GenerationRequest<'_>,
        out: &'o mut ImageMut<'_>,
    ) -> Result<GenerationResult<'o>, ProviderError> {
        request.validate()?;
        let provider = self.get(id).ok_or(ProviderError::UnknownProvider)?;
        let info = provider.info();
        if !info.capabilities().supports(request.kind) {
            return Err(ProviderError::Unsupported);
        }
        if !info.locality().is_local() && !request.allow_remote {
            return Err(ProviderError::RemoteNotAllowed);
        }
        run(provider, request, out)
    }
}

/// Checks the output size, runs the provider, assembles provenance.
/// `request` is already validated.
fn run<'o>(
    provider: &dyn TextureProvider,
    request: &GenerationRequest<'_>,
    out: &'o mut ImageMut<'_>,
) -> Result<GenerationResult<'o>, ProviderError> {
    let src = request.source;
    if out.width != src.width || out.height != src.height || out.data.len() != src.data.len() {
        return Err(ProviderError::OutputSizeMismatch);
    }
    provider.generate(request, out)?;
    let info = *provider.info();
    let data: &'o [u8] = out.data;
    Ok(GenerationResult {
        image: Image {
            width: src.width,
            height: src.height,
            data,
        },
        provenance: Provenance {
            provider_id: info.id(),
            model: info.model(),
            model_version: info.model_version(),
            locality: info.locality(),
            deterministic: info.deterministic(),
            kind: request.kind,
            seed: request.seed,
            strength: request.strength,
            width: src.width,
            height: src.height,
            prompt_digest: fnv1a64(request.prompt.as_bytes()),
            source_digest: fnv1a64(src.data),
            mask_digest: request.mask.map(fnv1a64),
            output_digest: fnv1a64(data),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock: masked texels become `[seed, strength*255, kind, 255]`,
    /// unmasked texels copy the source. No mask (seamless) = all texels.
    fn mock(req: &GenerationRequest<'_>, out: &mut ImageMut<'_>) -> Result<(), ProviderError> {
        let fill = [
            req.seed as u8,
            (req.strength * 255.0) as u8,
            req.kind as u8,
            255,
        ];
        for (t, (o, s)) in out
            .data
            .chunks_exact_mut(4)
            .zip(req.source.data.chunks_exact(4))
            .enumerate()
        {
            let hit = req.mask.is_none_or(|m| m[t] != 0);
            o.copy_from_slice(if hit { &fill[..] } else { s });
        }
        Ok(())
    }

    fn failing(_: &GenerationRequest<'_>, _: &mut ImageMut<'_>) -> Result<(), ProviderError> {
        Err(ProviderError::ProviderFailed)
    }

    /// Compile-time construction: the WASM-safe `static` pattern.
    static LOCAL: FnProvider = FnProvider::new(
        ProviderInfo::new(
            "local-inpaint",
            "mock-sd",
            "1.0",
            Locality::InProcess,
            Capabilities::INPAINT,
            true,
        ),
        mock,
    );
    static SIDECAR: FnProvider = FnProvider::new(
        ProviderInfo::new(
            "sidecar",
            "mock-comfy",
            "",
            Locality::LocalSidecar,
            Capabilities::INPAINT.union(Capabilities::SEAMLESS_TILE),
            false,
        ),
        mock,
    );
    static REMOTE: FnProvider = FnProvider::new(
        ProviderInfo::new(
            "remote-api",
            "mock-cloud",
            "2",
            Locality::Remote,
            Capabilities::INPAINT.union(Capabilities::SEAMLESS_TILE),
            false,
        ),
        mock,
    );
    static BROKEN: FnProvider = FnProvider::new(
        ProviderInfo::new(
            "broken",
            "mock-x",
            "",
            Locality::InProcess,
            Capabilities::INPAINT,
            true,
        ),
        failing,
    );

    /// 2×1 source: texel 0 = [1,2,3,4], texel 1 = [5,6,7,8].
    const SRC: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    fn src() -> Image<'static> {
        Image {
            width: 2,
            height: 1,
            data: &SRC,
        }
    }

    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn ids_follow_node_name_rules() {
        assert!(is_valid_id("sd15-inpaint_v2"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id("has space"));
        assert!(!is_valid_id("dot.ted"));
        assert!(is_valid_id(core::str::from_utf8(&[b'a'; 64]).unwrap()));
        assert!(!is_valid_id(core::str::from_utf8(&[b'a'; 65]).unwrap()));
    }

    #[test]
    #[should_panic(expected = "provider id must be")]
    fn info_rejects_bad_id_at_runtime() {
        let _ = ProviderInfo::new(
            "bad id",
            "m",
            "",
            Locality::InProcess,
            Capabilities::INPAINT,
            true,
        );
    }

    #[test]
    fn capabilities_bitset() {
        let both = Capabilities::INPAINT.union(Capabilities::SEAMLESS_TILE);
        assert!(both.supports(GenerationKind::Inpaint));
        assert!(both.supports(GenerationKind::SeamlessTile));
        assert!(!Capabilities::INPAINT.supports(GenerationKind::SeamlessTile));
        assert!(Capabilities::NONE.is_empty());
        assert!(!Locality::Remote.is_local());
        assert!(Locality::LocalSidecar.is_local());
    }

    #[test]
    fn request_defaults() {
        let r = GenerationRequest::inpaint(src(), &[0, 1]);
        assert_eq!(r.kind, GenerationKind::Inpaint);
        assert_eq!(r.seed, 0);
        assert_eq!(r.strength, 1.0);
        assert_eq!(r.prompt, "");
        assert!(!r.allow_remote);
        let t = GenerationRequest::seamless_tile(src());
        assert_eq!(t.mask, None);
        assert_eq!(t.validate(), Ok(()));
    }

    #[test]
    fn request_validation_errors_are_exact() {
        let ok = GenerationRequest::inpaint(src(), &[0, 255]);
        assert_eq!(ok.validate(), Ok(()));

        let mut bad = ok;
        bad.source.data = &[1, 2, 3, 4, 5, 6, 7];
        assert_eq!(bad.validate(), Err(ProviderError::BadSource));
        let mut zero = ok;
        zero.source.width = 0;
        zero.source.data = &[];
        assert_eq!(zero.validate(), Err(ProviderError::BadSource));

        for s in [f32::NAN, -0.01, 1.01, f32::INFINITY] {
            assert_eq!(
                ok.with_strength(s).validate(),
                Err(ProviderError::StrengthOutOfRange)
            );
        }
        assert_eq!(ok.with_strength(0.0).validate(), Ok(()));

        let mut nomask = ok;
        nomask.mask = None;
        assert_eq!(nomask.validate(), Err(ProviderError::MissingMask));
        assert_eq!(
            ok.with_mask(&[1, 1, 1]).validate(),
            Err(ProviderError::MaskSizeMismatch)
        );
        assert_eq!(
            ok.with_mask(&[0, 0]).validate(),
            Err(ProviderError::EmptyMask)
        );
        // A seamless request with a mask is held to the same mask rules.
        assert_eq!(
            GenerationRequest::seamless_tile(src())
                .with_mask(&[0])
                .validate(),
            Err(ProviderError::MaskSizeMismatch)
        );

        let bytes = [b'p'; MAX_PROMPT_LEN + 1];
        let long = core::str::from_utf8(&bytes).unwrap();
        assert_eq!(
            ok.with_prompt(long).validate(),
            Err(ProviderError::PromptTooLong)
        );
        assert_eq!(ok.with_prompt(&long[..MAX_PROMPT_LEN]).validate(), Ok(()));
    }

    #[test]
    fn registry_capacity_and_duplicates() {
        let mut reg: ProviderRegistry<'_, 2> = ProviderRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.capacity(), 2);
        assert_eq!(reg.register(&LOCAL), Ok(()));
        assert_eq!(reg.register(&LOCAL), Err(ProviderError::DuplicateId));
        assert_eq!(reg.register(&REMOTE), Ok(()));
        assert_eq!(reg.register(&SIDECAR), Err(ProviderError::RegistryFull));
        assert_eq!(reg.len(), 2);
        let ids: [&str; 2] = {
            let mut it = reg.iter().map(|p| p.info().id());
            [it.next().unwrap(), it.next().unwrap()]
        };
        assert_eq!(ids, ["local-inpaint", "remote-api"]);
        assert_eq!(reg.get("remote-api").unwrap().info().model(), "mock-cloud");
        assert!(reg.get("sidecar").is_none());
        let d: ProviderRegistry<'_, 3> = ProviderRegistry::default();
        assert_eq!((d.len(), d.capacity()), (0, 3));
    }

    #[test]
    fn resolve_is_local_first() {
        let mut reg: ProviderRegistry<'_, 4> = ProviderRegistry::new();
        reg.register(&REMOTE).unwrap();
        reg.register(&LOCAL).unwrap();
        reg.register(&SIDECAR).unwrap();
        let inpaint = GenerationRequest::inpaint(src(), &[1, 0]);
        // Remote registered first, yet local wins even when allowed.
        assert_eq!(reg.resolve(&inpaint).unwrap().info().id(), "local-inpaint");
        let allowed = reg.resolve(&inpaint.allowing_remote()).unwrap();
        assert_eq!(allowed.info().id(), "local-inpaint");
        // Only the sidecar (local) and remote support seamless.
        let tile = GenerationRequest::seamless_tile(src());
        assert_eq!(reg.resolve(&tile).unwrap().info().id(), "sidecar");

        let mut remote_only: ProviderRegistry<'_, 1> = ProviderRegistry::new();
        remote_only.register(&REMOTE).unwrap();
        assert_eq!(
            remote_only.resolve(&tile).err(),
            Some(ProviderError::RemoteNotAllowed)
        );
        assert_eq!(
            remote_only
                .resolve(&tile.allowing_remote())
                .unwrap()
                .info()
                .id(),
            "remote-api"
        );

        let mut inpaint_only: ProviderRegistry<'_, 1> = ProviderRegistry::new();
        inpaint_only.register(&LOCAL).unwrap();
        assert_eq!(
            inpaint_only.resolve(&tile).err(),
            Some(ProviderError::NoEligibleProvider)
        );
        let empty: ProviderRegistry<'_, 1> = ProviderRegistry::new();
        assert_eq!(
            empty.resolve(&inpaint).err(),
            Some(ProviderError::NoEligibleProvider)
        );
    }

    #[test]
    fn generate_writes_output_and_provenance() {
        let mut reg: ProviderRegistry<'_, 2> = ProviderRegistry::new();
        reg.register(&LOCAL).unwrap();
        let mask = [0u8, 255];
        let req = GenerationRequest::inpaint(src(), &mask)
            .with_seed(42)
            .with_strength(0.5)
            .with_prompt("rust");
        let mut buf = [0u8; 8];
        let mut out = ImageMut {
            width: 2,
            height: 1,
            data: &mut buf,
        };
        let res = reg.generate(&req, &mut out).unwrap();
        // Texel 0 unmasked → source; texel 1 → [42, 127, Inpaint=0, 255].
        let expected = [1, 2, 3, 4, 42, 127, 0, 255];
        assert_eq!(res.image.data, &expected);
        assert_eq!((res.image.width, res.image.height), (2, 1));
        let p = res.provenance;
        assert_eq!(
            p,
            Provenance {
                provider_id: "local-inpaint",
                model: "mock-sd",
                model_version: "1.0",
                locality: Locality::InProcess,
                deterministic: true,
                kind: GenerationKind::Inpaint,
                seed: 42,
                strength: 0.5,
                width: 2,
                height: 1,
                prompt_digest: fnv1a64(b"rust"),
                source_digest: fnv1a64(&SRC),
                mask_digest: Some(fnv1a64(&mask)),
                output_digest: fnv1a64(&expected),
            }
        );
        assert_ne!(p.output_digest, p.source_digest);
    }

    #[test]
    fn generate_with_names_the_provider() {
        let mut reg: ProviderRegistry<'_, 4> = ProviderRegistry::new();
        reg.register(&LOCAL).unwrap();
        reg.register(&SIDECAR).unwrap();
        reg.register(&REMOTE).unwrap();
        reg.register(&BROKEN).unwrap();
        let tile = GenerationRequest::seamless_tile(src()).with_seed(3);
        let mut buf = [0u8; 8];
        let mut out = ImageMut {
            width: 2,
            height: 1,
            data: &mut buf,
        };

        let res = reg.generate_with("sidecar", &tile, &mut out).unwrap();
        // No mask → every texel is [3, 255, SeamlessTile=1, 255].
        assert_eq!(res.image.data, &[3, 255, 1, 255, 3, 255, 1, 255]);
        assert_eq!(res.provenance.mask_digest, None);
        assert_eq!(res.provenance.locality, Locality::LocalSidecar);
        assert!(!res.provenance.deterministic);

        assert_eq!(
            reg.generate_with("nope", &tile, &mut out).err(),
            Some(ProviderError::UnknownProvider)
        );
        assert_eq!(
            reg.generate_with("local-inpaint", &tile, &mut out).err(),
            Some(ProviderError::Unsupported)
        );
        assert_eq!(
            reg.generate_with("remote-api", &tile, &mut out).err(),
            Some(ProviderError::RemoteNotAllowed)
        );
        let remote = reg
            .generate_with("remote-api", &tile.allowing_remote(), &mut out)
            .unwrap();
        assert_eq!(remote.provenance.provider_id, "remote-api");
        assert_eq!(remote.provenance.locality, Locality::Remote);

        let inpaint = GenerationRequest::inpaint(src(), &[1, 1]);
        assert_eq!(
            reg.generate_with("broken", &inpaint, &mut out).err(),
            Some(ProviderError::ProviderFailed)
        );
    }

    #[test]
    fn generate_rejects_before_calling_the_provider() {
        let mut reg: ProviderRegistry<'_, 1> = ProviderRegistry::new();
        reg.register(&BROKEN).unwrap();
        let mut buf = [0u8; 8];
        let mut out = ImageMut {
            width: 2,
            height: 1,
            data: &mut buf,
        };
        // Validation precedes the provider: EmptyMask, not ProviderFailed.
        let empty = GenerationRequest::inpaint(src(), &[0, 0]);
        assert_eq!(
            reg.generate(&empty, &mut out).err(),
            Some(ProviderError::EmptyMask)
        );
        let mut small = [0u8; 4];
        let mut wrong = ImageMut {
            width: 1,
            height: 1,
            data: &mut small,
        };
        let req = GenerationRequest::inpaint(src(), &[1, 0]);
        assert_eq!(
            reg.generate(&req, &mut wrong).err(),
            Some(ProviderError::OutputSizeMismatch)
        );
    }
}
