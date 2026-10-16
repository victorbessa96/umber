//! Ground-truth probe for the .spp import audit (2026-10-16): the LZ4
//! block decode and the assbin material-name pairing, against real
//! corpus bytes dumped by the python reference. Never ships.
use std::path::PathBuf;

fn main() {
    let tmp = PathBuf::from("/tmp");
    // 1. The material-name pairing on the real scene dump.
    let scene = std::fs::read(tmp.join("scene.assbin")).unwrap();
    let names = umber_spp::texture_cache::material_names(&scene);
    println!("material_names pairs: {}", names.len());
    for (uid, name) in &names {
        println!("  uid={uid} name={name:?}");
    }
    println!(
        "cache ids in the file: 1423614033083633785, 17594422824770056089, 8074553802099130621"
    );

    // 2. lz4_flex on the real block (dumped by the python reference).
    let block = std::fs::read(tmp.join("channel1.block")).unwrap();
    let mut out = vec![0u8; 67_108_864];
    match lz4_flex::block::decompress_into(&block, &mut out) {
        Ok(w) => println!("lz4_flex decompress_into: OK, wrote {w} of 67108864"),
        Err(e) => println!("lz4_flex decompress_into: ERR {e}"),
    }

    // 3. The reader's public decode() on the FULL raw payload (reconstructed
    //    = header map + u32 blen + block, exactly as h5py read it).
    let mut payload = Vec::new();
    // rebuild: map (count=6, key/values as measured), then u32 len, block.
    // Simpler: read the payload back through h5py once more is not possible
    // here; instead reconstruct from the block: the python script wrote only
    // the block. So: call decode() on the file path through umber_spp::open
    // and check the set-1 baseColor notes + pixel stats directly.
    let mut sets_ok = 0;
    let path = PathBuf::from(std::env::var("HOME").unwrap())
        .join("corpora/umber/spp/BessaCreatureV2.spp");
    match umber_spp::open(&path) {
        Ok(project) => {
            for set in &project.texture_sets {
                for ch in &set.channels {
                    let nonzero = ch
                        .pixels
                        .chunks_exact(4)
                        .filter(|t| t.iter().any(|&b| b != 0))
                        .count();
                    let total = ch.width as usize * ch.height as usize;
                    println!(
                        "reader: set={:?} ch={:<12} nonzero-texels={}/{} ({:.1}%)",
                        set.name,
                        ch.name,
                        nonzero,
                        total,
                        100.0 * nonzero as f64 / total as f64
                    );
                }
                sets_ok += 1;
            }
        }
        Err(e) => println!("open ERR: {e}"),
    }
    println!("sets walked: {sets_ok}");
}
