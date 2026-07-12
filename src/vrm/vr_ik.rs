//! HMD / コントローラ pose から VRM humanoid 骨格を駆動する VR IK。
//!
//! bevy_ash_xr の `ik/` から VRM 知識層 (2 ボーン解析 IK・腰推定・スパイン分配・
//! rest-pose キャリブレーション・骨への適用) を移管したもの。歩行サイクルなどの
//! 入力生成はアプリ側の責務で、[`VrIkTargets`] を通して毎フレーム受け取る。

pub mod solver;
