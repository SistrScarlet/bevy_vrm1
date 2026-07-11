# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`bevy_vrm1` is a Bevy plugin for loading and animating VRM 1.0 models and VRMA animations. It supports Spring Bone physics, LookAt gaze control, Node Constraints, and Expression systems following the official VRM specification update order.

**Important**: Only VRM 1.0 is supported. This crate is in early development and may undergo breaking changes.

## Development Commands

### Build and Check
```bash
# Check compilation
cargo check

# Build the project
cargo build

# Build with features
cargo build --features serde,log
```

### Testing
```bash
# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run tests with logging
cargo test --features log
```

### Running Examples
```bash
# Basic VRM loading
cargo run --example simple

# Spring bone physics demo
cargo run --example spring_bone

# LookAt demos
cargo run --example look_at_cursor
cargo run --example look_at_target

# VRMA animation playback
cargo run --example vrma
cargo run --example vrma_transition

# MToon multiple directional lights
cargo run --example multiple_lights
```

### Linting
The project uses Clippy with custom lints defined in `Cargo.toml`:
```bash
cargo clippy
```

## Architecture Overview

### Plugin Structure

The `VrmPlugin` is the main entry point. It is a thin composition of `VrmCorePlugin`
(all rendering-agnostic functionality) and `MtoonMaterialPlugin` (wgpu-based MToon
rendering). Apps with a custom renderer (no wgpu RenderPlugin) add `VrmCorePlugin`
directly instead — it also registers the reflect types needed for the MToon-less path
(e.g. `MeshMaterial3d<StandardMaterial>`).

```
VrmPlugin
├── VrmCorePlugin
│   ├── VrmLoaderPlugin          (Asset loading: .vrm files)
│   ├── VrmInitializePlugin      (VRM spawning & initialization)
│   ├── VrmDetachPlugin          (RequestDetachVrm)
│   ├── VrmSpringBonePlugin      (Spring physics)
│   ├── VrmHumanoidBonePlugin    (Bone hierarchy mapping, HumanoidBoneEntities)
│   ├── VrmExpressionPlugin      (Morph target expressions)
│   ├── VrmNodeConstraintPlugin  (VRMC_node_constraint support)
│   ├── LookAtPlugin             (Gaze control system)
│   ├── BodyTrackingPlugin       (LookAt-driven head-chain tracking)
│   └── BoneOverlayPlugin        (Additive bone rotation overlay)
└── MtoonMaterialPlugin          (Shader & material rendering, wgpu)
```

VRMA (animation) is a separate plugin (`VrmaPlugin`) that works alongside VrmPlugin.

### VRM Asset Loading Pipeline

1. **VrmHandle → VrmAsset**: User spawns entity with `VrmHandle`, loader creates `VrmAsset` from glTF
2. **Asset → Components**: Extracts VRM extensions and creates registries (`VrmExpressionRegistry`, `HumanoidBoneRegistry`, etc.)
3. **Delayed Initialization**: Waits for all bone entities to spawn, then triggers initialization events to wire up components

### Critical System Execution Order (VrmSystemSets)

The system execution order follows the [VRM specification](https://vrm.dev/api/api_update/):

```
Animation (Bevy standard)
    ↓
VrmSystemSets::Constraints
    ↓
VrmSystemSets::PropagateAfterConstraints (manual transform propagation)
    ↓
VrmSystemSets::GazeControl (LookAt)
    ↓
BoneOverlaySystems (additive bone rotation overlay)
    ↓
VrmSystemSets::Expressions
    ↓
VrmSystemSets::PropagateAfterExpressions (conditional propagation — runs only while a
                                          BoneRotationOverlay is active; otherwise empty)
    ↓
VrmSystemSets::SpringBone
    ↓
VrmSystemSets::DetermineRedraw (triggers RequestRedraw if needed)
    ↓
TransformSystems::Propagate (Bevy standard propagation)
```

**Important**: This order is guaranteed by an `app.configure_sets(PostUpdate, (...).chain())` call in `src/vrm.rs`, which also orders the whole chain `.after(AnimationSystems)` and `.before(TransformSystems::Propagate)` (the latter guarantees Transform writes inside the chain reach the rendered pose in the same frame). Do not rely on `.after()`/`.before()` against `PropagateAfterExpressions` alone — it can be empty, and ordering against an empty set creates no edges in Bevy.

### Key Architectural Patterns

#### 1. Registry Pattern
Metadata extracted from glTF extensions is stored in registry components (HashMap-based), allowing deferred binding when entities are spawned:
- `HumanoidBoneRegistry`: Maps `VrmBone` names to glTF node entities
- `VrmExpressionRegistry`: Maps expression names to morph target node info
- `NodeConstraintRegistry`: Maps constraint sources to destination entities

#### 2. RestTransform Baseline
Systems use stored `RestTransform`/`RestGlobalTransform` (captured at initialization) as a baseline to compute deltas. This enables multiple systems to read the same base state without conflicts.

#### 3. Event-Driven Initialization
Uses Bevy observers to trigger initialization when conditions are met:
- `RequestInitializeHumanoidBones`
- `RequestInitializeSpringBone`
- `RequestInitializeNodeConstraints`
- `RequestInitializeExpressions`

#### 4. VRMA Retargeting
VRMA maintains separate registries per skeleton and uses custom animation curves (`BoneRotationAnimationCurve`, `HipsTranslationAnimationCurve`) to retarget animations from VRMA skeleton to VRM skeleton.

## Component Constraints

### Node Constraint System

Three constraint types (all run in parallel during `VrmSystemSets::Constraints`):

- **RotationConstraint**: Transfers entire local rotation from source to destination (use case: sub-arms)
- **RollConstraint**: Transfers rotation around a specific axis only (use case: twist bones)
- **AimConstraint**: Rotates destination to face source (use case: clothing accessories)

All use spherical linear interpolation (slerp) based on weight parameter (0.0-1.0).

### Spring Bone Physics

- Runs **after** all pose changes in `VrmSystemSets::SpringBone`
- Uses Verlet integration for physics simulation
- Each `SpringRoot` contains a chain of joints with collision detection
- Center node defines reference frame for inertia calculations

### LookAt System

Two modes:
- **Cursor Mode**: Tracks mouse cursor position via camera ray casting
- **Target Mode**: Tracks a specific entity

Updates `Head`, `LeftEye`, `RightEye` bone rotations based on `LookAtProperties` ranges.

## Transform Propagation Strategy

Bevy's default `TransformPropagate` runs once in `PostUpdate`. This crate manually invokes transform propagation **once**, after Constraints:

1. **After Constraints** (`PropagateAfterConstraints`): Ensures constraint changes propagate to `GlobalTransform` before LookAt reads positions

This is implemented in `src/vrm.rs` using:
```rust
use bevy::transform::systems::{propagate_parent_transforms, sync_simple_transforms};

app.add_systems(
    PostUpdate,
    (sync_simple_transforms, propagate_parent_transforms)
        .chain()
        .in_set(VrmSystemSets::PropagateAfterConstraints)
);
```

**Fork note (bevy_ash_xr)**: Upstream ran a second full-scene propagation in `PropagateAfterExpressions` (per the VRM spec update order). This fork removes the unconditional version as a performance optimization (~1.1ms/frame with 50 VRMs), accepting a quality tradeoff: transforms written by `GazeControl` (LookAt eye-bone locals and BodyTracking head-chain rotations) are only picked up by Bevy's standard `PostUpdate` propagation — guaranteed to run after the VRM chain by the `.before(TransformSystems::Propagate)` edge on the chain — so they reach the rendered pose in the same frame, but `GlobalTransform`s of head-chain descendants (colliders, joint anchors) that SpringBone reads lag by at most one frame. Exception: while any `BoneRotationOverlay` has `weight > 0.0`, `BoneOverlayPlugin` runs a conditional propagation in `PropagateAfterExpressions` (see `src/vrm/bone_overlay.rs`), so SpringBone reads overlay-applied `GlobalTransform`s in the same frame; when no overlay is active the set is empty and costs nothing. Inter-set ordering is guaranteed by `configure_sets` (see above). If a future system writes `Transform`s that SpringBone must read in the same frame, extend the propagation condition (or re-add unconditional propagation to this set).

## Working with VRM Specifications

When modifying update order or system timing, always reference:
- [VRM Update Order Specification](https://vrm.dev/api/api_update/)
- [Spring Bone Specification](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_springBone-1.0/README.md)
- [Node Constraint Specification](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_node_constraint-1.0/README.md)
- [LookAt Specification](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm-1.0/lookAt.md)
- [VRMA Specification](https://github.com/vrm-c/vrm-specification/blob/master/specification/VRMC_vrm_animation-1.0/README.md)

## Version Compatibility

| bevy_vrm1 | bevy |
|-----------|------|
| 0.5.0 ~   | 0.18 |
| 0.4.0 ~   | 0.17 |
| 0.1.0 ~   | 0.16 |

Rust edition: 2024

## Module Organization

```
src/
├── lib.rs                  (Main exports)
├── system_set.rs           (VrmSystemSets enum)
├── system_param.rs         (Helper system params: ChildSearcher, ParentSearcher, etc.)
├── vrm/                    (VRM 1.0 implementation)
│   ├── loader.rs           (VrmAsset loading)
│   ├── initialize.rs       (VRM spawning logic)
│   ├── expressions.rs      (Expression registry)
│   ├── humanoid_bone.rs    (Bone mapping, HumanoidBoneEntities)
│   ├── humanoid_bone/capsule_fit.rs (Bone positions → capsule approximation)
│   ├── bone_overlay.rs     (Additive bone rotation overlay)
│   ├── look_at.rs          (Gaze control)
│   ├── spring_bone/        (Physics simulation)
│   ├── node_constraint/    (Constraint types)
│   ├── mtoon/              (Shader implementation)
│   └── gltf/               (glTF extension parsing)
└── vrma/                   (VRMA animation implementation)
    ├── loader.rs           (VRMA asset loading)
    ├── initialize.rs       (VRMA scene setup)
    └── animation/          (Retargeting system)
```

## Testing Notes

- Tests use `bevy_test_helper` for setting up minimal Bevy apps
- Test VRM models are in `assets/` (excluded from crate publication)
- Sample model credit: **AliciaSolid** by **© DWANGO Co., Ltd.**

## Common Pitfalls

1. **System Ordering**: When adding new VRM-related systems, always ensure they run in the correct `VrmSystemSets` and respect the specification order
2. **Transform Propagation**: If a system modifies `Transform` and another system needs to read `GlobalTransform` in the same frame, manual propagation may be needed
3. **Registry Dependencies**: Systems that need bone entities must run after `RequestInitializeHumanoidBones` completes
4. **Changed Filters**: Constraint systems use `Changed<Transform>` filters for performance; ensure source transforms are actually marked as changed

## Memory

memory の実体は repo 内 `docs/memory/` (harness auto-memory は無効化済 = `.claude/settings.json` の `autoMemoryEnabled: false`、旧 `~/.claude/projects/.../memory/` は削除済 = 書込禁止)。書込・更新は `.claude/rules/memory-format.md` の書式規律に従い手動実施。

@docs/memory/MEMORY.md
