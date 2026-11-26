# Kill brief: CLI/control security and lifecycle hardening

- Lock: `razul-cli-control-security`.
- Trigger: tools audit at `ec979743` found six independently reproduced defects in the current CLI/MCP/control path.
- Implementation lane: bounded coding claw; dragon verifies diff and all tests. No cargo on the claw side.

## Findings and required fixes

1. **Workspace confinement bypass.** `workspace.rs` only confines `mesh/in/input`, `out/out_dir/output`, and `dir/path`; `bake_transfer` uses `low/high`, asset import uses `file/library`. MCP and `serve` share this incomplete confiner. Omitting `library` still writes to the per-user asset directory without a write root. Implement one variant-aware confiner covering these reads/writes and require write authority for the default library, or explicitly separate/document an unconfined mode. Add negative tests for every path-bearing AgentStep variant.

2. **Direct MCP tool envelope is wrong.** `mcp.rs` direct `bake/export/paint_stroke/paint_snapshot/set_active/paint_read` serializes the inner body, not the `{name: body}` envelope. `set_active` reaches the agent parser as `step:"set"` and fails. Dispatch the full wrapped object and add direct tools/call coverage for every affected entry.

3. **Outer success on agent error.** `serve.rs` `sess.reply` and `steps` wrap inner errors in outer `ok:true`. Propagate the inner `ok` flag to the outer envelope and assert top-level failure for an agent-level error.

4. **Long-job caller timeout regression.** App control waits 30 minutes for `command.run`, but `agent.rs::control_call` hardcodes 30 seconds for every command. Apply the job-aware policy, with a small grace period over the server ceiling, and pin it with a delayed mock control server rather than a 30-minute sleep.

5. **Predictable token-file clobbering and permissions.** `umber-core/src/control.rs::write_token_file` follows symlinks and only sets 0600 on creation. Use atomic create-new/no-follow or private temporary-file+rename semantics, enforce owner-only permissions on every write, verify the opened file is regular, and never follow a pre-existing symlink. Test symlink and permissive-existing-file cases.

6. **Dead MCP child leaves bridge busy and client hanging.** `umber-app/src/control.rs` child-stdout EOF exits without clearing `BridgeState.out`, closing the client stream, or reaping the child. Coordinate both pump exits through one cleanup path that closes the client stream, kills/reaps the child, and clears `child` and `out`. Add a child-crash regression.

## Constraints

- One kill lock covering only the CLI/core/app control files, their direct tests, and this kill's report.
- Do not change the public agent schema, protocol version, release artifacts, upstream PRs, centralhost, or unrelated audit lanes.
- Preserve loopback token authentication and existing successful behavior. No silent unconfined write mode is added without explicit documentation.

## Verification

- Focused regression for each of the six findings.
- Full `umber-cli` suite, app control/bridge tests, workspace clippy, rustfmt, static gates, and an independent bounded review.
- Dragon commits and pushes via `safe-push.py` after verification, then dispatches current-tip CI.
