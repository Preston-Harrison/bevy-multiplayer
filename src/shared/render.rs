/// Used implicitly by all entities without a `RenderLayers` component.
/// Our world model camera and all objects other than the player are on this layer.
/// The light source belongs to both layers.
pub const DEFAULT_RENDER_LAYER: usize = 0;

/// Used by the view model camera and the player's arm.
/// The light source belongs to both layers.
// pub const VIEW_MODEL_RENDER_LAYER: usize = 1;

/// The player camera uses this ordering.
pub const DEFAULT_CAMERA_ORDER: isize = 0;

/// UI Elements are rendered on top of the game.
pub const UI_CAMERA_ORDER: isize = 1;
