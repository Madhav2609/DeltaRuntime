# DeltaRuntime

DeltaRuntime is a profile-based runtime manager for moddable games.  
It introduces a **workspace overlay** on top of a clean base installation and builds isolated runtimes by linking files from a **global, content-addressed cache**.

Right now, the focus is on **GTA: San Andreas**, where DeltaRuntime works as a **GTA Runtime Manager (GRM)**. The system is not game-specific, and once it is stable for GTA it can be generalized to support other moddable titles.

---

## Core Concepts

### Profiles
Each profile represents a self-contained environment.  
A profile includes:
- A **workspace** (`profiles/<name>/workspace`) for mods and overrides.  
- A **saves directory** (`profiles/<name>/saves`), junctioned into the game’s save path.  
- Metadata such as creation date and last used time.  

Profiles let you maintain separate setups such as “Vanilla”, “Overhaul”, or “CLEO-only” without duplicating the game.

### Workspace Overlay
The workspace mirrors the base game’s folder structure:  
- Files in the workspace override base files.  
- Deletions are tracked with tombstones.  
- In the UI, users always see a virtual game folder combining base + overlay.  

### Global Cache
All files are hashed (BLAKE3) and stored once in a global cache:  
- Identical files across profiles share the same blob.  
- Different versions of the same filename (e.g. `handling.cfg`) produce distinct blobs.  
- This keeps runtimes space-efficient and avoids duplication.  

### Runtime Builder
When launching a profile:
1. Hardlink the base installation into a temporary runtime.  
2. Overlay the profile’s winning files (from the cache).  
3. Finalize atomically and launch the game.  

This allows:
- Fast runtime builds (linking instead of copying).  
- Minimal disk growth.  
- Incremental rebuilds that only touch changed files.  

### Per-Profile Saves
Each profile has its own saves. A junction replaces the default GTA save directory, so switching profiles also switches visible saves. On first run, the original saves are backed up.

### Safety and Recovery
- Base install is never modified.  
- All writes are atomic.  
- Stale runtimes are removed on startup.  
- Garbage collection removes unreferenced blobs after a grace period.  

---

## Current Scope

The immediate development target is **GTA: San Andreas** because:
- It has a long history of heavy modding.  
- Many users keep multiple full copies for different setups.  
- Existing tools (like ModLoader) help, but don’t manage multiple installs.  

DeltaRuntime complements ModLoader by making it possible to maintain multiple parallel ModLoader setups, without duplicating the game.

---

## Future Directions

Although GTA is the starting point, DeltaRuntime is designed to be **game-agnostic**.  
Planned future applications include:  
- Skyrim, Fallout, or similar moddable RPGs.  
- Minecraft modpacks and configurations.  
- Any moddable game where users juggle multiple installs.  
- Even non-gaming scenarios like QA, configuration testing, or sandboxed labs.  

---

## Project Status

DeltaRuntime is currently experimental and under active development.  
The short-term goal is to validate and stabilize the GTA workflow before extending to other games.

---

## Contributing

This is currently a solo project by a 16-year-old developer experimenting with Rust and Tauri while solving a practical problem with GTA modding. Contributions, feedback, and testing ideas are welcome via issues and discussions.

---

