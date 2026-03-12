# Rustaria — Technical Documentation

A voxel engine written in Rust, rendering a procedurally generated 3D world using wgpu and winit.

---

## Table of Contents

1. [Project Structure](#project-structure)
2. [Architecture Overview](#architecture-overview)
3. [Crate: `rustaria-core`](#crate-rustaria-core)
   - [Block System](#block-system)
   - [Chunk System](#chunk-system)
   - [Mesh Generation](#mesh-generation)
   - [Terrain Generation](#terrain-generation)
   - [World Manager](#world-manager)
4. [Crate: `rustaria-game`](#crate-rustaria-game)
   - [Renderer](#renderer)
   - [Camera](#camera)
   - [Input System](#input-system)
   - [Pipeline](#pipeline)
   - [Shader (WGSL)](#shader-wgsl)
   - [GPU Mesh](#gpu-mesh)
   - [Frustum Culling](#frustum-culling)
   - [Debug Overlay](#debug-overlay)
   - [App (main.rs)](#app-mainrs)
   - [GameState](#gamestate)
   - [Streaming](#streaming)
   - [Render](#render)
5. [Controls](#controls)
6. [Build & Run](#build--run)
7. [Data Flow](#data-flow)

---

## Project Structure

```
rustaria/
├── Cargo.toml                      # Workspace root
├── rustaria-core/                  # Engine logic (no GPU dependency)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── block.rs                # Block types, IDs, registry
│       ├── blocks/
│       │   ├── mod.rs
│       │   └── defaults.rs         # Default block registration (AIR, STONE, DIRT, GRASS, WATER, BEDROCK)
│       ├── chunk.rs                # Chunk storage (Dense / Compressed stub)
│       ├── mesh.rs                 # CPU-side mesh builder with neighbor culling + parallel meshing
│       ├── world.rs                # Procedural terrain generator (Perlin noise, caves, water, bedrock)
│       └── world_manager.rs        # Chunk lifecycle, HashMap storage, parallel generation
└── rustaria-game/                  # Windowed application (wgpu + winit)
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # Entry point, App, ApplicationHandler, event loop
        ├── game_state.rs           # GameState struct definition and initialization
        ├── streaming.rs            # Chunk streaming: load/unload, priority queue, budget
        ├── render.rs               # Per-frame render loop (GameState::render)
        ├── renderer.rs             # wgpu device/surface/queue init
        ├── camera.rs               # FPS camera (yaw/pitch, mouse look, WASD)
        ├── input.rs                # Keyboard + mouse input state machine
        ├── pipeline.rs             # Render pipelines, uniforms, day/night cycle
        ├── frustum.rs              # View-frustum culling for chunk draw calls
        ├── gpu_mesh.rs             # Per-chunk GPU buffer wrapper
        ├── debug.rs                # Debug overlay (wireframe, chunk borders)
        └── shader.wgsl             # WGSL vertex + fragment shaders (diffuse + ambient)
```

---

## Architecture Overview

The project is split into two crates with a strict separation of concerns:

| Crate | Role | GPU dependency |
|---|---|---|
| `rustaria-core` | Game logic: blocks, chunks, mesh building, terrain generation, world management | None |
| `rustaria-game` | Rendering: wgpu pipeline, winit window, FPS camera, input, event loop | wgpu, winit |

`rustaria-game` depends on `rustaria-core`. The core crate produces plain Rust data structures (`Vec<Vertex>`, `Vec<u32>`, `ChunkData`) that the game crate uploads to the GPU and renders.

The game crate is split into focused modules:

| Module | Responsibility |
|---|---|
| `main.rs` | App shell, winit event loop (~85 lines) |
| `game_state.rs` | GameState struct + async initialization |
| `streaming.rs` | Chunk streaming: priority queue, load/unload, parallel gen + mesh |
| `render.rs` | Full per-frame render logic |

---

## Crate: `rustaria-core`

### Block System

**Files:** `rustaria-core/src/block.rs`, `rustaria-core/src/blocks/defaults.rs`

#### `BlockId`

```rust
pub struct BlockId(pub u16);
```

A newtype wrapper around `u16` identifying a block type. Predefined constants:

| Constant | Value | Description |
|---|---|---|
| `BlockId::AIR` | 0 | Empty block (not rendered) |
| `BlockId::STONE` | 1 | Stone |
| `BlockId::DIRT` | 2 | Dirt |
| `BlockId::GRASS` | 3 | Grass |
| `BlockId::WATER` | 4 | Water |
| `BlockId::BEDROCK` | 5 | Bedrock (indestructible base layer) |

`is_air()` returns `true` when the ID is `0`. The mesh builder uses this to skip rendering and face culling checks.

#### `BlockColor`

```rust
pub struct BlockColor { pub r: f32, pub g: f32, pub b: f32 }
```

RGB color in the `[0.0, 1.0]` range. Implements `From<BlockColor> for [f32; 3]` for easy interop with the mesh builder.

#### `Block` trait

```rust
pub trait Block {
    fn id(&self) -> BlockId;
    fn is_solid(&self) -> bool;
    fn color(&self) -> BlockColor;
}
```

Defines the interface a block type must expose. The concrete implementation is `BlockType`.

#### `BlockType`

A concrete struct implementing `Block`:

| Field | Type | Description |
|---|---|---|
| `id` | `BlockId` | Unique identifier |
| `name` | `&'static str` | Human-readable name |
| `color` | `BlockColor` | Base RGB color |
| `solid` | `bool` | Whether it blocks movement / triggers face culling |

#### `BlockRegistry`

```rust
pub struct BlockRegistry { blocks: Vec<BlockType> }
```

A flat `Vec`-based registry where the index equals `BlockId.0`. AIR **must** be registered first (index 0).

| Method | Description |
|---|---|
| `new()` | Creates a registry and calls `blocks::defaults::register_defaults` to populate all default blocks |
| `register(block)` | Appends a block and returns its `BlockId` |
| `get(id)` | Returns `Option<&BlockType>` by ID |

---

### Chunk System

**File:** `rustaria-core/src/chunk.rs`

#### Constants

```rust
pub const CHUNK_SIZE: usize = 16;
pub const CHUNK_VOLUME: usize = 16 * 16 * 16; // 4096
```

A chunk is a 16×16×16 cube of blocks.

#### `StorageMode`

```rust
pub enum StorageMode {
    Dense(Box<[BlockId; CHUNK_VOLUME]>),
    Compressed,
}
```

| Variant | Description |
|---|---|
| `Dense` | Full 4096-element array on the heap. Used for all current operations. |
| `Compressed` | Placeholder for future Palette+RLE compression. Not yet implemented (`todo!()`). |

#### `ChunkData`

```rust
pub struct ChunkData {
    pub storage: StorageMode,
    pub position: (i32, i32, i32), // chunk coordinates (not world coordinates)
    pub dirty: bool,                // true when blocks have been modified since last mesh build
}
```

| Method | Description |
|---|---|
| `new(position)` | Creates an all-air chunk at the given chunk coordinate |
| `get(x, y, z)` | Returns the `BlockId` at local coordinates `[0, CHUNK_SIZE)` |
| `set(x, y, z, block)` | Sets a block and marks the chunk as dirty |

**Index formula:** `index = x + y * CHUNK_SIZE + z * CHUNK_SIZE²`

---

### Mesh Generation

**File:** `rustaria-core/src/mesh.rs`

#### `Vertex`

```rust
#[repr(C)]
#[derive(Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 3],
    pub normal:   [f32; 3],
    pub ao:       f32,
}
```

`repr(C)` + `bytemuck::Pod` allows zero-copy casting to `&[u8]` for GPU upload. Total size: **40 bytes** per vertex (10 × f32).

| Field | Description |
|---|---|
| `position` | World-space position (chunk offset applied) |
| `color` | RGB block color from the registry |
| `normal` | Face normal vector (used for diffuse lighting in the shader) |
| `ao` | Ambient occlusion factor (currently always `1.0`, reserved for future use) |

#### `ChunkNeighbors`

```rust
pub struct ChunkNeighbors<'a> {
    pub pos_x: Option<&'a ChunkData>,
    pub neg_x: Option<&'a ChunkData>,
    pub pos_y: Option<&'a ChunkData>,
    pub neg_y: Option<&'a ChunkData>,
    pub pos_z: Option<&'a ChunkData>,
    pub neg_z: Option<&'a ChunkData>,
}
```

Optional references to the 6 neighboring chunks. Used by `mesh_chunk` for inter-chunk hidden-face culling at chunk boundaries. When a neighbor is `None` (not yet loaded), the face is emitted as a safe default.

`ChunkNeighbors::empty()` creates a struct with all neighbors set to `None`.

#### `mesh_chunk` algorithm

```rust
pub fn mesh_chunk(
    chunk: &ChunkData,
    registry: &BlockRegistry,
    neighbors: &ChunkNeighbors,
) -> (Vec<Vertex>, Vec<u32>)
```

Implements **culled meshing** with inter-chunk neighbor awareness:

1. Iterate every block in the chunk (z → y → x order).
2. Skip AIR blocks.
3. For each of the 6 faces of a solid block, check the neighboring block:
   - **Inside chunk:** If neighbor is AIR → emit face, otherwise skip.
   - **At chunk boundary:** Look up the neighbor chunk via `ChunkNeighbors`. If the neighbor chunk exists, sample the wrapped local coordinate using `rem_euclid`. If no neighbor is loaded, emit the face (safe default).
4. Each emitted face includes:
   - 4 vertices with world-space position (chunk offset applied), block color, face normal, and AO = 1.0.
   - 6 indices forming two counter-clockwise triangles.

#### `mesh_chunks_parallel`

```rust
pub fn mesh_chunks_parallel(
    positions: &[(i32, i32, i32)],
    world: &WorldManager,
    registry: &BlockRegistry,
) -> Vec<((i32, i32, i32), Vec<Vertex>, Vec<u32>)>
```

Meshes multiple chunks in parallel using Rayon. Each chunk is meshed independently with its 6 neighbors fetched from the `WorldManager`. Returns a flat `Vec` of `(position, vertices, indices)` tuples. Unknown block IDs fall back to magenta `[1.0, 0.0, 1.0]` for visual debugging.

---

### Terrain Generation

**File:** `rustaria-core/src/world.rs`

#### `TerrainGenerator`

```rust
pub struct TerrainGenerator {
    perlin: Perlin,
}
```

Procedural terrain generator using the `noise` crate's Perlin noise.

| Method | Description |
|---|---|
| `new(seed)` | Creates a generator with a given seed |
| `height_at(world_x, world_z)` | Returns terrain height (0..48) for a world-space column |
| `generate_chunk(cx, cy, cz)` | Generates a full 16³ chunk at the given chunk coordinates |

#### Heightmap algorithm

The height at each (x, z) column is computed from 4 layers of Perlin noise:

1. **Continent mask** (scale 0.005): Low-frequency noise determining plains vs mountains regions. Values below 0.4 produce flat plains, above 0.6 produce full mountains, with a smooth blend in between.
2. **3 octaves of detail noise** (scales 0.02, 0.05, 0.1): Combined with weights 1.0, 0.5, 0.25 and normalized to a 0..1 range.

The continent blend interpolates between a narrow height range (plains) and the full range (mountains).

#### Block placement

For each column, blocks are placed based on distance from the surface and world constants:

| Condition | Block |
|---|---|
| `world_y > height` and `world_y <= SEA_LEVEL` | WATER (ocean fill) |
| `world_y > height` | AIR |
| `world_y == height` (above sea level) | GRASS |
| `world_y == height` (at/below sea level) | DIRT (submerged surface) |
| `world_y > height - 4` | DIRT |
| `world_y > BEDROCK_HEIGHT` | STONE |
| `world_y <= BEDROCK_HEIGHT` | BEDROCK |

Caves are carved using 3D Perlin noise: blocks below the terrain surface are removed when the cave noise value exceeds a threshold, creating connected underground voids.

---

### World Manager

**File:** `rustaria-core/src/world_manager.rs`

#### `WorldManager`

```rust
pub struct WorldManager {
    chunks: HashMap<(i32, i32, i32), ChunkData>,
    generator: TerrainGenerator,
}
```

Manages chunk lifecycle: generation, storage, dirty-flag propagation, and parallel bulk generation.

| Method | Description |
|---|---|
| `new(seed)` | Creates a new world with a seeded `TerrainGenerator` |
| `get_chunk(cx, cy, cz)` | Returns `Option<&ChunkData>` |
| `has_chunk(cx, cy, cz)` | Returns whether the chunk is loaded |
| `generate_chunk(cx, cy, cz)` | Generates and inserts one chunk, marks 6 neighbors dirty |
| `generate_chunks_parallel(positions)` | Generates multiple chunks in parallel via Rayon, then inserts all and marks neighbors dirty |
| `get_dirty_chunks()` | Returns all chunk positions currently marked dirty |
| `clear_dirty(pos)` | Clears the dirty flag for a chunk after re-meshing |
| `unload_chunk(cx, cy, cz)` | Removes a chunk from memory |
| `chunk_count()` | Returns the number of loaded chunks |

When a new chunk is generated, all 6 adjacent chunks (if loaded) are marked dirty so they can be re-meshed with correct inter-chunk face culling along the shared boundary.

---

## Crate: `rustaria-game`

### Renderer

**File:** `rustaria-game/src/renderer.rs`

Wraps the wgpu initialization sequence into a single `Renderer` struct.

```rust
pub struct Renderer {
    pub surface:               wgpu::Surface<'static>,
    pub device:                wgpu::Device,
    pub queue:                 wgpu::Queue,
    pub config:                wgpu::SurfaceConfiguration,
    pub is_surface_configured: bool,
    pub window:                Arc<Window>,
}
```

#### Initialization sequence (`Renderer::new`)

```
Instance (selects backend: Vulkan / Metal / DX12)
  └─> Surface (attached to the winit Window)
        └─> Adapter (physical GPU handle, high-performance preference)
              └─> Device + Queue (logical connection)
                    └─> SurfaceConfiguration (sRGB format preferred, VSync/Fifo)
```

The surface is **not configured** during `new()`. It is configured on the first `WindowEvent::Resized` event via `resize()`. `wgpu::Features::POLYGON_MODE_LINE` is requested to support wireframe rendering.

#### `resize(width, height)`

Reconfigures the surface with new dimensions. Sets `is_surface_configured = true` which unblocks the render loop. Also triggers depth buffer and camera aspect ratio updates in `GameState`.

---

### Camera

**File:** `rustaria-game/src/camera.rs`

#### `CameraUniform`

```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],  // 4×4 MVP matrix, column-major
}
```

Uploaded to the GPU each frame as a uniform buffer at `@group(0) @binding(0)`.

#### `Camera`

A free-look FPS camera controlled by keyboard (position) and mouse (orientation).

| Parameter | Default value |
|---|---|
| Initial position | `(128.0, 60.0, 128.0)` |
| FOV | 70° vertical |
| Near / Far | 0.1 / 512.0 |
| Move speed | 0.6 units/frame |
| Mouse sensitivity | 0.002 rad/pixel |

| Method | Description |
|---|---|
| `new(width, height)` | Creates the camera with defaults |
| `resize(width, height)` | Updates the aspect ratio on window resize |
| `build_uniform()` | Builds the `CameraUniform` using `glam::Mat4::look_at_rh` and `perspective_rh` |
| `update(input)` | Applies WASD movement and mouse look. Pitch clamped to ±89°. |
| `upload(queue, buffer)` | Writes the current `view_proj` matrix to the GPU buffer |

---

### Input System

**File:** `rustaria-game/src/input.rs`

#### `InputState`

```rust
#[derive(Default)]
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub mouse_captured: bool,
    pub toggle_light: bool,
    pub regen_world: bool,
}
```

Movement keys are held-state booleans. Mouse deltas are accumulated between frames and reset to zero after each frame. `toggle_light` and `regen_world` are one-shot flags consumed once per frame.

| Input | Action |
|---|---|
| Left click | Captures the mouse (enables mouse look) |
| Escape | Releases the mouse, or exits if already free |
| G | Toggles wireframe debug mode |
| B | Toggles chunk border debug overlay |
| L | Sets `toggle_light` flag (day/night toggle) |
| R | Sets `regen_world` flag (world regeneration) |
| W/Z/↑ | Forward movement |
| S/↓ | Backward movement |
| A/← | Left movement |
| D/→ | Right movement |
| Space | Move up |
| Shift | Move down |

---

### Pipeline

**File:** `rustaria-game/src/pipeline.rs`

#### `PipelineBundle`

```rust
pub struct PipelineBundle {
    pub fill_pipeline:         wgpu::RenderPipeline,
    pub wireframe_pipeline:    wgpu::RenderPipeline,
    pub chunk_border_pipeline: wgpu::RenderPipeline,
    pub camera_bind_group:     wgpu::BindGroup,
    pub camera_buffer:         wgpu::Buffer,
    pub light_buffer:          wgpu::Buffer,
}
```

#### Vertex buffer layout

Describes the `Vertex` struct to wgpu (stride = 40 bytes, 10 × f32):

| Attribute | Shader location | Format | Offset |
|---|---|---|---|
| position | 0 | `Float32x3` | 0 bytes |
| color | 1 | `Float32x3` | 12 bytes |
| normal | 2 | `Float32x3` | 24 bytes |
| ao | 3 | `Float32` | 36 bytes |

#### Pipelines

`create_pipeline` returns three `wgpu::RenderPipeline` sharing the same shader and bind group:

| Pipeline | `PolygonMode` | Back-face culling | Usage |
|---|---|---|---|
| Fill | `Fill` | Enabled (`Back`) | Normal rendering |
| Wireframe | `Line` | Disabled | Debug mode (key G) |
| Chunk border | `Line` | Disabled | Debug chunk boundary overlay (key B) |

#### Bind group layout

| Binding | Stage | Content |
|---|---|---|
| 0 | Vertex | `CameraUniform` — view_proj matrix |
| 1 | Fragment | `LightUniform` — sun direction, ambient, sun color |

#### Day/Night Cycle

`day_night_light(time)` computes the `LightUniform` for a time value in `[0.0, 1.0)`:

| Time | Meaning |
|---|---|
| 0.0 | Dawn |
| 0.25 | Noon (default) |
| 0.5 | Dusk |
| 0.75 | Midnight |

`sky_color(time)` returns a matching `wgpu::Color` for the render pass clear color.

---

### Shader (WGSL)

**File:** `rustaria-game/src/shader.wgsl`

The vertex shader applies the MVP matrix. The fragment shader computes a simple directional light model:

```
light_total = ambient + max(dot(normal, sun_dir), 0.0) * 0.8
final_color = vertex_color * light_total * ao
```

---

### GPU Mesh

**File:** `rustaria-game/src/gpu_mesh.rs`

```rust
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer:  wgpu::Buffer,
    pub num_indices:   u32,
}
```

A lightweight wrapper owning the GPU vertex and index buffers for a single chunk. `GameState` stores these in a `HashMap<(i32, i32, i32), GpuMesh>`. Chunks with no visible faces are removed from the map.

---

### Frustum Culling

**File:** `rustaria-game/src/frustum.rs`

```rust
pub struct Frustum { planes: [Vec4; 6] }
```

Extracts the 6 clip planes from the view-projection matrix each frame. Before issuing a draw call for a chunk mesh, `Frustum::contains_chunk(cx, cy, cz)` checks whether the chunk's AABB intersects the frustum. Chunks entirely outside are skipped, which dramatically reduces draw calls at high render distances.

---

### Debug Overlay

**File:** `rustaria-game/src/debug.rs`

```rust
pub struct DebugOverlay {
    wireframe:     bool,
    chunk_borders: bool,
}
```

| Method | Description |
|---|---|
| `new()` | Creates the overlay with all flags disabled |
| `toggle_wireframe()` | Flips the wireframe flag |
| `toggle_chunk_borders()` | Flips the chunk border flag |
| `show_wireframe()` | Read by the render loop to select the active pipeline |
| `show_chunk_borders()` | Read by the render loop to draw chunk AABB lines |

When chunk borders are enabled, loaded chunks are outlined:
- **Yellow** — chunk has a GPU mesh (contains visible blocks)
- **Cyan** — chunk is loaded but produced no geometry (fully air or culled)

---

### App (main.rs)

**File:** `rustaria-game/src/main.rs`

The entry point (~85 lines). Declares all modules and implements `winit::application::ApplicationHandler` for the `App` struct.

#### World Constants

```rust
pub const RENDER_DISTANCE: i32 = 16;       // chunks in each horizontal direction
pub const WORLD_HEIGHT: i32 = 3;           // cy range: -WORLD_DEPTH .. WORLD_HEIGHT
pub const WORLD_DEPTH: i32 = 4;            // cy starts at -4 (world_y = -64)
pub const GEN_BUDGET_PER_FRAME: usize = 32;
pub const MESH_BUDGET_PER_FRAME: usize = 16;
```

#### Event handling

| Event | Action |
|---|---|
| `resumed` | Create window, init `GameState` via `pollster::block_on` |
| `CloseRequested` | Exit event loop |
| `Resized` | Resize surface + recreate depth buffer + update camera aspect ratio |
| `RedrawRequested` | Call `state.render()` |
| Keyboard/Mouse | Delegated to `input::handle_keyboard` |
| `DeviceEvent` (mouse motion) | Delegated to `input::handle_device_event` |

---

### GameState

**File:** `rustaria-game/src/game_state.rs`

Owns all GPU resources, game data, and subsystems. Key fields:

| Field | Type | Description |
|---|---|---|
| `renderer` | `Renderer` | wgpu device/surface/queue |
| `gpu_meshes` | `HashMap<(i32,i32,i32), GpuMesh>` | One entry per loaded chunk with geometry |
| `render_pipeline` | `wgpu::RenderPipeline` | Normal fill rendering |
| `wireframe_pipeline` | `wgpu::RenderPipeline` | Debug wireframe |
| `chunk_border_pipeline` | `wgpu::RenderPipeline` | Debug chunk AABB lines |
| `camera` | `Camera` | FPS camera |
| `world` | `WorldManager` | Chunk storage + terrain generator |
| `registry` | `BlockRegistry` | Block type lookup |
| `pending_queue` | `VecDeque<(i32,i32,i32)>` | Ordered chunk load queue |
| `loaded_chunks` | `HashSet<(i32,i32,i32)>` | Currently loaded chunk positions |
| `last_cam_chunk` | `Option<(i32,i32,i32)>` | Last camera chunk position (for delta detection) |
| `fps` | `f64` | Exponential moving average FPS |

`GameState::new` is async (required by wgpu) and called synchronously via `pollster::block_on`.

---

### Streaming

**File:** `rustaria-game/src/streaming.rs`

Implements `GameState::update_streaming` and `GameState::load_chunks`, plus two helper functions.

#### `update_streaming`

Called at the start of each `load_chunks`. Runs only when the camera moves to a new chunk coordinate:

1. Compute the desired chunk set: all `(cx, cy, cz)` within `RENDER_DISTANCE` horizontally and `WORLD_DEPTH..WORLD_HEIGHT` vertically.
2. Unload chunks that fell outside the desired set (removes from `loaded_chunks`, `WorldManager`, and `gpu_meshes`).
3. Sort new chunks by Manhattan distance from the camera chunk (closest first) and push into `pending_queue`.

#### `load_chunks`

Called every frame, budget-capped to limit frame time:

1. Pop up to `GEN_BUDGET_PER_FRAME` positions from `pending_queue`.
2. Generate them in parallel via `WorldManager::generate_chunks_parallel`.
3. Build a mesh queue: all newly generated chunks + up to `MESH_BUDGET_PER_FRAME` additional dirty chunks (neighbor re-meshes).
4. Mesh them in parallel via `mesh::mesh_chunks_parallel`.
5. Upload results to GPU as `GpuMesh` entries. Chunks that produced no geometry are removed from `gpu_meshes`.

#### Helper functions

```rust
pub fn camera_chunk_pos(camera: &Camera) -> (i32, i32, i32)
pub fn compute_desired_set(cam_cx: i32, cam_cz: i32) -> HashSet<(i32, i32, i32)>
```

---

### Render

**File:** `rustaria-game/src/render.rs`

Implements `GameState::render`, called every `WindowEvent::RedrawRequested`:

1. `request_redraw()` to sustain continuous rendering.
2. Update FPS counter (exponential moving average: 90% old + 10% new).
3. Guard: skip if surface not yet configured.
4. Call `load_chunks()` for progressive streaming.
5. Handle one-shot inputs: `toggle_light` (L), `regen_world` (R — creates new `WorldManager`, clears all GPU state).
6. Update camera from input, reset mouse deltas, upload camera uniform.
7. Compute and upload light uniform from `day_time`.
8. Acquire swapchain texture. On `Lost`/`Outdated`, reconfigure and skip frame.
9. Begin render pass with `sky_color(day_time)` clear and depth clear 1.0.
10. Build `Frustum` from current view-proj matrix.
11. Iterate `gpu_meshes`, skip chunks failing frustum test, `draw_indexed` the rest.
12. If chunk borders debug is enabled, emit line geometry for all loaded chunk AABBs.
13. Update window title with FPS, position, and draw call counts.
14. Submit command buffer and present.

---

## Controls

| Key | Action |
|---|---|
| `W` / `Z` / `↑` | Move forward |
| `S` / `↓` | Move backward |
| `A` / `←` | Move left |
| `D` / `→` | Move right |
| `Space` | Move up |
| `Shift` | Move down |
| Mouse (when captured) | Look around (yaw/pitch) |
| Left click | Capture mouse |
| `G` | Toggle wireframe debug mode |
| `B` | Toggle chunk border overlay |
| `L` | Toggle day/night |
| `R` | Regenerate world (new random seed) |
| `Escape` | Release mouse, or quit if mouse is already free |

---

## Build & Run

**Prerequisites:** Rust toolchain (stable), a GPU with Vulkan / Metal / DX12 support.

```bash
cd rustaria
cargo run -p rustaria-game
```

Enable logging:

```bash
RUST_LOG=info cargo run -p rustaria-game
```

Release build (recommended for performance):

```bash
cargo run -p rustaria-game --release
```

The world seed is logged at startup. Press `R` to regenerate with a new seed.

---

## Data Flow

```
WorldManager::new(seed)
  └─> TerrainGenerator::new(seed)

Each frame (progressive streaming):
  pending_queue.pop_front() × GEN_BUDGET     ← priority queue, closest first
        │
        ▼
  WorldManager::generate_chunks_parallel()   ← Rayon parallel generation
  → ChunkData (Dense, 4096 BlockIds)
  → marks 6 neighbors dirty per new chunk
        │
        ▼
  mesh::mesh_chunks_parallel()               ← Rayon parallel meshing
  → Vec<Vertex>, Vec<u32>                       with inter-chunk culling
        │
        ▼
  GpuMesh::new(device, vertices, indices)    ← wgpu upload to GPU VRAM
  → stored in HashMap<(i32,i32,i32), GpuMesh>

Each frame (render):
  Frustum::from_view_proj(view_proj)
  Camera::update(input) + upload()
  day_night_light(time) → light uniform
  get_current_texture()
  begin_render_pass(sky_color)
  set_pipeline()   ← fill or wireframe
  for each gpu_mesh:
    if frustum.contains_chunk() → draw_indexed()
  [optional] chunk border overlay lines
  submit() + present()
```
