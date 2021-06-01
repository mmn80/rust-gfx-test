// @[semantic("POSITION")]
layout (location = 0) in vec3 in_pos;

// @[semantic("NORMAL")]
layout (location = 1) in vec3 in_normal;

// w component is a sign value (-1 or +1) indicating handedness of the tangent basis
// see GLTF spec for more info
// @[semantic("TANGENT")]
layout (location = 2) in vec4 in_tangent;

// @[semantic("TEXCOORD")]
layout (location = 3) in vec2 in_uv;

// @[semantic("MODELMATRIX")]
layout (location = 4) in mat4 in_model_matrix; // Uses locations 4-7. The semantic will be named `MODELMATRIX0` through `MODELMATRIX3`.
// layout (location = 5) in mat4 in_model_matrix;
// layout (location = 6) in mat4 in_model_matrix;
// layout (location = 7) in mat4 in_model_matrix;

// Do all math in view space so that it is more easily portable to deferred/clustered
// forward rendering (vs = view space)
layout (location = 0) out vec3 out_position_vs;
layout (location = 1) out vec3 out_normal_vs;
layout (location = 2) out vec3 out_tangent_vs;
layout (location = 3) out vec3 out_binormal_vs;
layout (location = 4) out vec2 out_uv;

// for shadows
layout (location = 5) out vec4 out_position_ws;

layout (location = 6) out mat3 out_model_view;

void pbr_main() {
    mat4 model_view_proj = per_view_data.view_proj * in_model_matrix;
    mat4 model_view = per_view_data.view * in_model_matrix;

    gl_Position = model_view_proj * vec4(in_pos, 1.0);
    out_position_vs = (model_view * vec4(in_pos, 1.0)).xyz;

    // NOTE: Not sure if I need to normalize after the matrix multiply
    out_normal_vs = mat3(model_view) * in_normal;
    out_tangent_vs = mat3(model_view) * in_tangent.xyz;

    vec3 binormal = cross(in_normal, in_tangent.xyz) * in_tangent.w;
    out_binormal_vs = mat3(model_view) * binormal;

    out_uv = in_uv;

    // Used to sample the shadow map
    out_position_ws = in_model_matrix * vec4(in_pos, 1.0);

    out_model_view = mat3(model_view);
}