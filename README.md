# Engram-Transform

This repo tracks the implementation of Engram-Transform, a decentralized transformation engine for AI-training data.

Currently, it is in Phase 0 (Deterministic executor).

---

## Phase 0: Deterministic executor

In this phase, I focus on implementing a platform to execute data transformation algorithms, with 4 reference programs: `count`, `filter`, `histogram`, and `checksum`. Each of these programs is defined through a specification, written in `engram/Spec.md`. Implementations are written in Rust, compiled to WASM, and executed through a host program written in Python. All of the programs must be deterministically executed. The programs must be run according to a fuel budget, and automatically halted if exceed its budget. Programs also left an execution trace, which ultimately decide whether or not they are deterministic.

---

## Structure

The repository consists of 3 main directories:
- `engram` contains various notes of mine, and the specification for reference programs.
- `executor` contains the host program, written in Python. Read `executor/README.txt` for more information.
- `guest_program` contains the reference programs. Build with this command (assuming from the root of the repo): `cargo build --target wasm32-unknown-unknown`.
