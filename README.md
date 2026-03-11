# Rustaria Prototype — Technical Documentation

A voxel engine prototype written in Rust, rendering a 3D chunk of blocks using wgpu and winit.

---

## Table of Contents

1. [Project Structure](#project-structure)
2. [Architecture Overview](#architecture-overview)
3. [Crate: `rustaria-core`](#crate-rustaria-core)
   - [Block System](#block-system)
   - [Chunk System](#chunk-system)
   - [Mesh Generation](#mesh-generation)
4. [Crate: `rustaria-game`](#crate-rustaria-game)
   - [Renderer](#renderer)
   - [Pipeline](#pipeline)
   - [Shader (WGSL)](#shader-wgsl)
   - [Debug Overlay](#debug-overlay)
   - [App & GameState (main.rs)](#app--gamestate-mainrs)
5. [Controls](#controls)
6. [Build & Run](#build--run)
7. [Data Flow](#data-flow)

---

## Project Structure

```
rustaria/
├── Cargo.toml                  # Workspace root
├── rustaria-core/              # Engine logic (no GPU dependency)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── block.rs            # Block types, IDs, registry
│       ├── chunk.rs            # Chunk storage and test generators
│       └── mesh.rs             # CPU-side mesh builder
└── rustaria-game/              # Windowed application (wgpu + winit)
    ├── Cargo.toml
    └── src/
        ├── main.rs             # Entry point, App, GameState, event loop
        ├── renderer.rs         # wgpu device/surface/queue init
        ├── pipeline.rs         # Render pipelines, camera uniform
        ├── debug.rs            # Debug overlay (wireframe toggle)
        └── shader.wgsl         # WGSL vertex + fragment shaders
```

---

## Architecture Overview

The project is split into two crates with a strict separation of concerns:

| Crate | Role | GPU dependency |
|---|---|---|
| `rustaria-core` | Game logic: blocks, chunks, mesh building | None |
| `rustaria-game` | Rendering: wgpu pipeline, winit window, event loop | wgpu, winit |

`rustaria-game` depends on `rustaria-core`. The core crate produces plain Rust data structures (`Vec<Vertex>`, `Vec<u32>`) that the game crate uploads to the GPU.

---

## Crate: `rustaria-core`

### Block System

**File:** [rustaria-core/src/block.rs](rustaria/rustaria-core/src/block.rs)

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
| `new()` | Creates a registry pre-populated with AIR, STONE, DIRT, GRASS |
| `register(block)` | Appends a block and returns its `BlockId` |
| `get(id)` | Returns `Option<&BlockType>` by ID |

---

### Chunk System

**File:** [rustaria-core/src/chunk.rs](rustaria/rustaria-core/src/chunk.rs)

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
| `Compressed` | Placeholder for future run-length encoding or palette compression. Not yet implemented (`todo!()`). |

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
| `generate_single_block_test()` | Creates a chunk with one STONE block at (0,0,0); used for quick tests |
| `generate_flat_test()` | Creates a flat terrain layer: STONE at y=0, DIRT at y=1..3, GRASS at y=4 |

**Index formula:** `index = x + y * CHUNK_SIZE + z * CHUNK_SIZE²`

---

### Mesh Generation

**File:** [rustaria-core/src/mesh.rs](rustaria/rustaria-core/src/mesh.rs)

#### `Vertex`

```rust
#[repr(C)]
#[derive(Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 3],
}
```

`repr(C)` + `bytemuck::Pod` allows zero-copy casting to `&[u8]` for GPU upload. Total size: **24 bytes** per vertex.

#### `mesh_chunk` algorithm

```rust
pub fn mesh_chunk(chunk: &ChunkData, registry: &BlockRegistry) -> (Vec<Vertex>, Vec<u32>)
```

Implements **naive face culling** (also called "culled meshing"):

1. Iterate every block in the chunk (z → y → x order).
2. Skip AIR blocks.
3. For each of the 6 faces of a solid block, check the neighboring block:
   - If the neighbor is AIR **or** out-of-bounds → emit a quad (4 vertices + 6 indices).
   - Otherwise → face is hidden, skip it.
4. Apply a **static lighting multiplier** per face direction to give a sense of depth without a lighting shader:

| Face | Direction | Light multiplier |
|---|---|---|
| Top | +Y | 1.0 (brightest) |
| Sides | ±X, ±Z | 0.75 |
| Bottom | -Y | 0.5 (darkest) |

Each quad is two counter-clockwise triangles (winding order matches `FrontFace::Ccw` in the pipeline):

```
0──1
│\ │
│ \│
3──2

Indices: [0,1,2,  0,2,3]
```

Unknown block IDs (not in registry) fall back to magenta `[1.0, 0.0, 1.0]` for easy visual debugging.

---

## Crate: `rustaria-game`

### Renderer

**File:** [rustaria-game/src/renderer.rs](rustaria/rustaria-game/src/renderer.rs)

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

Reconfigures the surface with new dimensions. Sets `is_surface_configured = true` which unblocks the render loop. Also called in `GameState` to recreate the depth buffer.

---

### Pipeline

**File:** [rustaria-game/src/pipeline.rs](rustaria/rustaria-game/src/pipeline.rs)

#### `CameraUniform`

```rust
struct CameraUniform {
    view_proj: [[f32; 4]; 4],  // 4×4 MVP matrix, column-major
}
```

A **static** view-projection matrix computed once at startup. The camera is placed at `(8, 18, 28)` looking at `(8, 2, 8)` — above and in front of the 16×16 chunk — with a 70° vertical FOV.

The matrix is computed as `proj * view` where:
- `view` = right-handed look-at (`glam::Mat4::look_at_rh`)
- `proj` = right-handed perspective (`glam::Mat4::perspective_rh`, near=0.1, far=100.0)

Uploaded to the GPU as a uniform buffer accessible at `@group(0) @binding(0)` in the vertex shader.

#### Vertex buffer layout

Describes the `Vertex` struct to wgpu (stride = 24 bytes):

| Attribute | Shader location | Format | Offset |
|---|---|---|---|
| position | 0 | `Float32x3` | 0 bytes |
| color | 1 | `Float32x3` | 12 bytes |

#### `create_pipeline` — dual pipeline

Returns two `wgpu::RenderPipeline` and one `wgpu::BindGroup`:

| Pipeline | `PolygonMode` | Back-face culling | Usage |
|---|---|---|---|
| Fill | `Fill` | Enabled (`Back`) | Normal rendering |
| Wireframe | `Line` | Disabled | Debug mode (key G) |

Both pipelines share the same shader, bind group layout, and vertex buffer layout. Only `primitive.polygon_mode` and `primitive.cull_mode` differ.

Depth testing is enabled on both pipelines: `CompareFunction::Less`, `Depth32Float` format.

#### `create_depth_texture_view`

Creates a `Depth32Float` texture matching the surface dimensions. Must be recreated every time the window is resized (called from `GameState` on `WindowEvent::Resized`).

---

### Shader (WGSL)

**File:** [rustaria-game/src/shader.wgsl](rustaria/rustaria-game/src/shader.wgsl)

```wgsl
// Uniform
@group(0) @binding(0) var<uniform> camera: CameraUniform;

// Vertex shader
@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
}

// Fragment shader
@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
```

The shader is intentionally minimal: the vertex stage applies the MVP matrix, the fragment stage outputs the vertex color as-is. Lighting is pre-baked into vertex colors by `mesh_chunk`.

---

### Debug Overlay

**File:** [rustaria-game/src/debug.rs](rustaria/rustaria-game/src/debug.rs)

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

**File:** [rustaria-game/src/main.rs](rustaria/rustaria-game/src/main.rs)

#### `App`

Implements `winit::application::ApplicationHandler`. Holds an `Option<GameState>` that is populated in `resumed()` (the winit 0.30 replacement for the old `EventLoop::run` closure).

#### `GameState`

Owns all GPU resources and game data:

```rust
pub struct GameState {
    renderer:           Renderer,
    vertex_buffer:      wgpu::Buffer,
    index_buffer:       wgpu::Buffer,
    num_indices:        u32,
    render_pipeline:    wgpu::RenderPipeline,
    wireframe_pipeline: wgpu::RenderPipeline,
    camera_bind_group:  wgpu::BindGroup,
    depth_texture_view: wgpu::TextureView,
    debug:              DebugOverlay,
}
```

**Initialization (`GameState::new`)** runs asynchronously (required by wgpu's `request_adapter`), executed synchronously via `pollster::block_on`:

1. Initialize `Renderer` (device, surface, queue).
2. Build `BlockRegistry` and generate a flat test chunk (`generate_flat_test`).
3. Run `mesh_chunk` to produce vertex and index data on the CPU.
4. Upload vertex buffer and index buffer to the GPU.
5. Create render pipelines and camera bind group via `pipeline::create_pipeline`.
6. Create depth buffer via `pipeline::create_depth_texture_view`.

**Render loop (`GameState::render`)**:

1. Call `window.request_redraw()` to sustain continuous rendering.
2. Guard: skip if surface is not yet configured.
3. Acquire the next swapchain texture (`get_current_texture`). On `Lost`/`Outdated` errors, reconfigure the surface and skip the frame.
4. Create a `CommandEncoder`.
5. Begin a `RenderPass` with:
   - Clear color: `#010409` (dark navy, Rustaria brand color)
   - Depth clear: `1.0`
6. Bind the active pipeline (fill or wireframe based on `debug.wireframe`).
7. Draw with `draw_indexed`.
8. Submit the encoded commands and present the frame.

#### Event handling summary

| Event | Action |
|---|---|
| `CloseRequested` | Exit event loop |
| `KeyCode::Escape` pressed | Exit event loop |
| `KeyCode::KeyG` pressed | Toggle wireframe via `debug.toggle_wireframe()` |
| `WindowEvent::Resized` | Resize surface + recreate depth buffer |
| `WindowEvent::RedrawRequested` | Call `state.render()` |

---

## Controls

| Key | Action |
|---|---|
| `G` | Toggle wireframe debug mode |
| `Escape` | Quit |

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

---

## Data Flow

```
BlockRegistry::new()
ChunkData::generate_flat_test()
        │
        ▼
  mesh_chunk()                      ← rustaria-core (CPU only)
  → Vec<Vertex>, Vec<u32>
        │
        ▼
  create_buffer_init()              ← wgpu upload to GPU VRAM
  vertex_buffer, index_buffer
        │
        ▼
  Every frame:
  get_current_texture()
  begin_render_pass()
  set_pipeline()   ← fill or wireframe
  draw_indexed()
  submit() + present()
```
