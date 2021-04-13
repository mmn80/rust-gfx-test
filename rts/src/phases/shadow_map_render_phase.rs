use rafx::nodes::RenderPhase;
use rafx::nodes::{RenderPhaseIndex, SubmitNode};

rafx::declare_render_phase!(
    ShadowMapRenderPhase,
    SHADOW_MAP_RENDER_PHASE_INDEX,
    shadow_map_render_phase_sort_submit_nodes
);

#[profiling::function]
fn shadow_map_render_phase_sort_submit_nodes(mut submit_nodes: Vec<SubmitNode>) -> Vec<SubmitNode> {
    // Sort by distance from camera front to back
    log::trace!(
        "Sort phase {}",
        ShadowMapRenderPhase::render_phase_debug_name()
    );
    submit_nodes.sort_unstable_by(|a, b| a.distance().partial_cmp(&b.distance()).unwrap());

    submit_nodes
}
