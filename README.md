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
   - [Debug Overlay](#debug-overlay)
   - [App & GameState (main.rs)](#app--gamestate-mainrs)
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
│       │   └── defaults.rs         # Default block registration (AIR, STONE, DIRT, GRASS)
│       ├── chunk.rs                # Chunk storage (Dense / Compressed stub)
│       ├── mesh.rs                 # CPU-side mesh builder with neighbor culling
│       ├── world.rs                # Procedural terrain generator (Perlin noise)
│       └── world_manager.rs        # Chunk lifecycle & HashMap storage
└── rustaria-game/                  # Windowed application (wgpu + winit)
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # Entry point, App, GameState, event loop
        ├── renderer.rs             # wgpu device/surface/queue init
        ├── camera.rs               # FPS camera (yaw/pitch, mouse look, WASD)
        ├── input.rs                # Keyboard + mouse input state machine
        ├── pipeline.rs             # Render pipelines, uniforms, day/night cycle
        ├── gpu_mesh.rs             # Per-chunk GPU buffer wrapper
        ├── debug.rs                # Debug overlay (wireframe toggle)
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
| `new()` | Creates a registry and calls `blocks::defaults::register_defaults` to populate AIR, STONE, DIRT, GRASS |
| `register(block)` | Appends a block and returns its `BlockId` |
| `get(id)` | Returns `Option<&BlockType>` by ID |

Default block registration is separated into `blocks/defaults.rs`, keeping the registry logic clean and making it easy to add new block sets later.

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
| `generate_single_block_test()` | Creates a chunk with one STONE block at (0,0,0); legacy test |
| `generate_flat_test()` | Creates a flat terrain layer: STONE at y=0, DIRT at y=1..3, GRASS at y=4; legacy test |

**Index formula:** `index = x + y * CHUNK_SIZE + z * CHUNK_SIZE²`

Note: The test generators (`generate_single_block_test`, `generate_flat_test`) are defined in `world.rs` as `impl ChunkData` blocks for backward compatibility. Active world generation uses `TerrainGenerator` instead.

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

Face definitions are stored in a const `FACES` array of `FaceDef` structs, each containing the neighbor direction, 4 corner offsets, and the face normal vector.

Unknown block IDs (not in registry) fall back to magenta `[1.0, 0.0, 1.0]` for easy visual debugging.

---

### Terrain Generation

**File:** `rustaria-core/src/world.rs`

#### `TerrainGenerator`

```rust
pub struct TerrainGenerator {
    perlin: Perlin,
}
```

Procedural terrain generator using the `noise` crate's Perlin noise. Generates chunks independently from a 2D heightmap.

| Method | Description |
|---|---|
| `new(seed)` | Creates a generator with a given seed |
| `height_at(world_x, world_z)` | Returns terrain height (0..48) for a world-space column |
| `generate_chunk(cx, cy, cz)` | Generates a full 16³ chunk at the given chunk coordinates |

#### Heightmap algorithm

The height at each (x, z) column is computed from 4 layers of Perlin noise:

1. **Continent mask** (scale 0.005): Low-frequency noise that determines plains vs mountains regions. Values below 0.4 produce flat plains, above 0.6 produce full mountains, with a smooth blend in between.
2. **3 octaves of detail noise** (scales 0.02, 0.05, 0.1): Combined with weights 1.0, 0.5, 0.25 and normalized to a 0..1 range.

The continent blend interpolates between a narrow height range (plains: ~8–14 blocks) and the full range (mountains: 0–48 blocks).

#### Block placement

For each column, blocks are placed based on distance from the surface:

| Condition | Block |
|---|---|
| `world_y > height` | AIR |
| `world_y == height` | GRASS |
| `world_y > height - 4` | DIRT |
| `world_y <= height - 4` | STONE |

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

Manages chunk lifecycle: generation, storage, and neighbor dirty-flag propagation.

| Method | Description |
|---|---|
| `new(seed)` | Creates a new world with a `TerrainGenerator` seeded from the given value |
| `get_chunk(cx, cy, cz)` | Returns `Option<&ChunkData>` for the chunk at the given coordinates |
| `has_chunk(cx, cy, cz)` | Returns whether the chunk is loaded |
| `generate_chunk(cx, cy, cz)` | Generates the chunk, inserts it, and marks all 6 existing neighbors as dirty. Returns the list of dirty neighbor positions. |
| `chunk_count()` | Returns the number of loaded chunks |

When a new chunk is generated, all 6 adjacent chunks (if they exist) are marked dirty so they can be re-meshed with correct inter-chunk face culling along the shared boundary.

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

The surface is **not configured** during `new()`. It is configured on the first `WindowEvent::Resized` event via `resize()`. This is required by the learn-wgpu pattern and avoids a race condition on some platforms.

`wgpu::Features::POLYGON_MODE_LINE` is explicitly requested to support wireframe rendering.

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

```rust
pub struct Camera {
    pub position: glam::Vec3,
    pub yaw: f32,   // radians
    pub pitch: f32,  // radians
    aspect: f32,
    fov_y: f32,
    near: f32,
    far: f32,
}
```

A free-look FPS camera controlled by keyboard (position) and mouse (orientation).

| Parameter | Default value |
|---|---|
| Initial position | `(128.0, 60.0, 128.0)` — center of the 16×16 chunk world, above terrain |
| Initial yaw | 180° (looking toward -X) |
| Initial pitch | -20° (slightly looking down) |
| FOV | 70° vertical |
| Near / Far | 0.1 / 512.0 |
| Move speed | 0.6 units/frame |
| Mouse sensitivity | 0.002 rad/pixel |

| Method | Description |
|---|---|
| `new(width, height)` | Creates the camera with defaults and computes the aspect ratio |
| `resize(width, height)` | Updates the aspect ratio on window resize |
| `forward()` | Computes the look direction vector from yaw and pitch |
| `right()` | Computes the right vector (perpendicular to forward in the horizontal plane) |
| `build_uniform()` | Builds the `CameraUniform` with `proj * view` using `glam::Mat4::look_at_rh` and `perspective_rh` |
| `update(input)` | Applies WASD movement and mouse look from `InputState`. Pitch is clamped to ±89°. |
| `upload(queue, buffer)` | Writes the current `view_proj` matrix to the GPU uniform buffer |

#### `build_camera_buffer`

Standalone function that creates the GPU uniform buffer (`UNIFORM | COPY_DST`) with initial camera data. The `COPY_DST` usage allows per-frame updates via `queue.write_buffer`.

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

Holds the current state of all inputs. Movement keys are held-state booleans. Mouse deltas are accumulated between frames and reset to zero after each frame. `toggle_light` and `regen_world` are one-shot flags consumed once per frame.

#### `handle_keyboard`

Processes `WindowEvent` keyboard and mouse button events. Returns `true` if the event was consumed (closing, debug toggle, etc.).

| Input | Action |
|---|---|
| Left click | Captures the mouse (enables mouse look) |
| Escape | Releases the mouse if captured, otherwise exits the application |
| G | Toggles wireframe debug mode |
| L | Sets `toggle_light` flag (day/night toggle) |
| R | Sets `regen_world` flag (world regeneration) |
| W/Z/↑ | Forward movement |
| S/↓ | Backward movement |
| A/← | Left movement |
| D/→ | Right movement |
| Space | Move up |
| Shift | Move down |

#### `handle_device_event`

Captures raw mouse delta from `DeviceEvent::MouseMotion`. Only accumulates when `mouse_captured` is true. Called from `ApplicationHandler::device_event` in `main.rs`.

---

### Pipeline

**File:** `rustaria-game/src/pipeline.rs`

#### `PipelineBundle`

```rust
pub struct PipelineBundle {
    pub fill_pipeline:      wgpu::RenderPipeline,
    pub wireframe_pipeline: wgpu::RenderPipeline,
    pub camera_bind_group:  wgpu::BindGroup,
    pub camera_buffer:      wgpu::Buffer,
    pub light_buffer:       wgpu::Buffer,
}
```

Groups all pipeline-related GPU resources returned by `create_pipeline`.

#### Vertex buffer layout

Describes the `Vertex` struct to wgpu (stride = 40 bytes, 10 × f32):

| Attribute | Shader location | Format | Offset |
|---|---|---|---|
| position | 0 | `Float32x3` | 0 bytes |
| color | 1 | `Float32x3` | 12 bytes |
| normal | 2 | `Float32x3` | 24 bytes |
| ao | 3 | `Float32` | 36 bytes |

#### `create_pipeline` — dual pipeline

Returns two `wgpu::RenderPipeline` and the shared bind group with two uniform bindings:

| Pipeline | `PolygonMode` | Back-face culling | Usage |
|---|---|---|---|
| Fill | `Fill` | Enabled (`Back`) | Normal rendering |
| Wireframe | `Line` | Disabled | Debug mode (key G) |

Both pipelines share the same shader, bind group layout, and vertex buffer layout. Only `primitive.polygon_mode` and `primitive.cull_mode` differ.

Depth testing is enabled on both pipelines: `CompareFunction::Less`, `Depth32Float` format.

#### Bind group layout

| Binding | Stage | Content |
|---|---|---|
| 0 | Vertex | `CameraUniform` — view_proj matrix (updated each frame) |
| 1 | Fragment | `LightUniform` — sun direction, ambient, sun color (updated each frame) |

#### Day/Night Cycle

##### `day_night_light(time) -> [f32; 8]`

Computes the `LightUniform` data for a given time in the day cycle:

| Time value | Meaning |
|---|---|
| 0.0 | Dawn |
| 0.25 | Noon |
| 0.5 | Dusk |
| 0.75 | Midnight |

The sun direction rotates in a circle (`cos`/`sin` of `time * TAU`). Ambient light, sun color warmth, and intensity are all derived from the sun's vertical position. At night, ambient drops to 0.03 and sun contribution fades to zero.

##### `sky_color(time) -> wgpu::Color`

Returns a clear color matching the day/night state. Transitions from deep navy at night to warm blue at noon, with a warm tint near dawn/dusk.

#### `create_depth_texture_view`

Creates a `Depth32Float` texture matching the surface dimensions. Must be recreated every time the window is resized.

---

### Shader (WGSL)

**File:** `rustaria-game/src/shader.wgsl`

```wgsl
// Uniforms
@group(0) @binding(0) var<uniform> camera: CameraUniform;  // view_proj matrix
@group(0) @binding(1) var<uniform> light:  LightUniform;   // sun direction, ambient, sun color

// Vertex shader: applies MVP transform, passes color/normal/ao to fragment
@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color  = in.color;
    out.normal = in.normal;
    out.ao     = in.ao;
}

// Fragment shader: diffuse + ambient lighting
@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let ambient     = light.ambient;
    let diffuse     = max(dot(in.normal, light.sun_dir), 0.0);
    let light_total = ambient + diffuse * 0.8;
    let final_color = in.color * light_total * in.ao;
    return vec4<f32>(final_color, 1.0);
}
```

The vertex shader applies the MVP matrix and passes per-vertex attributes through. The fragment shader computes a simple directional light model: ambient light (constant base from `LightUniform`) plus diffuse contribution (dot product of face normal and sun direction, scaled by 0.8). The result is multiplied by vertex color and the AO factor.

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

A lightweight wrapper that owns the GPU vertex and index buffers for a single chunk. Created by uploading the output of `mesh_chunk` via `wgpu::util::DeviceExt::create_buffer_init`.

`GameState` stores these in a `HashMap<(i32, i32, i32), GpuMesh>` keyed by chunk coordinates. Chunks with no visible faces (empty mesh) are removed from the map.

---

### Debug Overlay

**File:** `rustaria-game/src/debug.rs`

```rust
pub struct DebugOverlay {
    pub wireframe: bool,
}
```

| Method | Description |
|---|---|
| `new()` | Creates the overlay with wireframe disabled |
| `toggle_wireframe()` | Flips the `wireframe` flag and logs the new state via `log::info!` |

The `wireframe` flag is read each frame in `GameState::render()` to select between the fill pipeline and the wireframe pipeline.

Designed to be extended with additional debug flags (e.g. `show_chunk_borders`, `show_normals`).

---

### App & GameState (main.rs)

**File:** `rustaria-game/src/main.rs`

#### `App`

Implements `winit::application::ApplicationHandler` (winit 0.30 pattern). Holds an `Option<GameState>` that is populated in `resumed()`.

#### World Constants

```rust
const WORLD_CX: i32 = 16;  // 16 chunks along X
const WORLD_CY: i32 = 4;   // 4 chunks along Y
const WORLD_CZ: i32 = 16;  // 16 chunks along Z
const CHUNKS_PER_FRAME: usize = 4;
```

The world is a 16×4×16 grid of chunks (256×64×256 blocks). Chunks are generated progressively at 4 chunks per frame to avoid startup lag.

#### `GameState`

Owns all GPU resources, game data, and subsystems:

```rust
pub struct GameState {
    renderer:           Renderer,
    gpu_meshes:         HashMap<(i32, i32, i32), GpuMesh>,
    render_pipeline:    wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    camera:             Camera,
    camera_buffer:      wgpu::Buffer,
    camera_bind_group:  wgpu::BindGroup,
    day_time:           f32,
    is_night:           bool,
    light_buffer:       wgpu::Buffer,
    input:              InputState,
    depth_texture_view: wgpu::TextureView,
    debug:              DebugOverlay,
    world:              WorldManager,
    registry:           BlockRegistry,
    pending_chunks:     Vec<(i32, i32, i32)>,
}
```

#### Initialization (`GameState::new`)

Runs asynchronously (required by wgpu's `request_adapter`), executed synchronously via `pollster::block_on`:

1. Initialize `Renderer` (device, surface, queue).
2. Build `BlockRegistry` with default blocks.
3. Create `WorldManager` with a random seed (derived from system time).
4. Build a sorted list of all chunk positions to load, sorted by distance to the camera's initial position (closest chunks are loaded first via `pop()`).
5. Create render pipelines, uniform buffers, and bind group via `pipeline::create_pipeline`.
6. Create the FPS `Camera` and depth buffer.

#### Progressive Chunk Loading (`load_chunks`)

Called every frame during `render()`. Each frame:

1. Pop up to `CHUNKS_PER_FRAME` positions from `pending_chunks`.
2. For each, call `world.generate_chunk()` which returns a list of dirty neighbors.
3. Mesh the new chunk and all affected neighbors by calling `mesh_and_upload()`.

`mesh_and_upload(cx, cy, cz)` retrieves the chunk and its 6 neighbors from the `WorldManager`, runs `mesh_chunk`, and either inserts the resulting `GpuMesh` into `gpu_meshes` or removes the entry if the mesh is empty.

#### Render loop (`GameState::render`)

1. Call `window.request_redraw()` to sustain continuous rendering.
2. Guard: skip if surface is not yet configured.
3. Run `load_chunks()` for progressive world loading.
4. Handle one-shot inputs: `toggle_light` (L key) toggles `is_night` and sets `day_time` to 0.75 or 0.25. `regen_world` (R key) creates a new `WorldManager` with a fresh random seed, clears all GPU meshes, and re-queues all chunk positions.
5. Update camera from input state, reset mouse deltas, upload camera uniform.
6. Compute and upload light uniform from `day_time`.
7. Acquire the next swapchain texture. On `Lost`/`Outdated` errors, reconfigure the surface and skip the frame.
8. Begin a render pass with sky color from `sky_color(day_time)` and depth clear 1.0.
9. Bind the active pipeline (fill or wireframe based on `debug.wireframe`).
10. Iterate all `gpu_meshes` and `draw_indexed` each one.
11. Submit commands and present.

#### Event handling summary

| Event | Action |
|---|---|
| `CloseRequested` | Exit event loop |
| `Resized` | Resize surface + recreate depth buffer + update camera aspect ratio |
| `RedrawRequested` | Call `state.render()` |
| Keyboard/Mouse | Delegated to `input::handle_keyboard` |
| `DeviceEvent` (mouse motion) | Delegated to `input::handle_device_event` |

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

The world seed is logged at startup. Press `R` to regenerate with a new seed.

---

## Data Flow

```
WorldManager::new(seed)
  └─> TerrainGenerator::new(seed)

Each frame (progressive loading):
  pending_chunks.pop()              ← sorted closest-first
        │
        ▼
  TerrainGenerator::generate_chunk()
  → ChunkData (Dense, 4096 BlockIds)
        │
        ▼
  WorldManager inserts chunk
  → marks 6 neighbors dirty
        │
        ▼
  mesh_chunk(chunk, registry, neighbors)   ← rustaria-core (CPU only)
  → Vec<Vertex>, Vec<u32>                     with inter-chunk culling
        │
        ▼
  GpuMesh::new(device, vertices, indices)  ← wgpu upload to GPU VRAM
  → stored in HashMap<(i32,i32,i32), GpuMesh>
        │
        ▼
  Every frame (render):
  Camera::update(input) + upload()
  day_night_light(time) → light uniform
  get_current_texture()
  begin_render_pass(sky_color)
  set_pipeline()   ← fill or wireframe
  for each gpu_mesh:
    draw_indexed()
  submit() + present()
```
