use bevy::prelude::SystemSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Ord, PartialOrd, Clone, Copy)]
pub enum VrmSystemSets {
    /// Node constraints processing.
    Constraints,

    /// Manual transform propagation after Constraints.
    /// This propagates Transform changes from Constraints to `GlobalTransform`.
    PropagateAfterConstraints,

    /// Look-at binding processing.
    GazeControl,

    /// Expression binding processing.
    Expressions,

    /// Manual transform propagation after Expressions.
    /// This propagates Transform changes from `GazeControl` and Expressions to `GlobalTransform`.
    ///
    /// `fork(bevy_ash_xr)`: 常時の propagation は登録されない。`BoneRotationOverlay` が
    /// アクティブなフレームのみ条件付き propagation が走る (詳細は `src/vrm.rs` の
    /// fork コメント参照)。この set は空になり得るため、順序はこの set への
    /// `.after()`/`.before()` ではなく `VrmCorePlugin` の `configure_sets` chain が担保する。
    PropagateAfterExpressions,

    /// This is used for spring bones.
    SpringBone,

    /// This is used to determine whether to send a [`RequestRedraw`](bevy::window::RequestRedraw).
    DetermineRedraw,
}
