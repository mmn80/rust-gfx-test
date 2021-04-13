#include <metal_stdlib>
#include <simd/simd.h>

using namespace metal;

struct PointLight
{
    float3 position_ws;
    float3 position_vs;
    float4 color;
    float range;
    float intensity;
    int shadow_map;
    char _m0_final_padding[4];
};

struct DirectionalLight
{
    float3 direction_ws;
    float3 direction_vs;
    float4 color;
    float intensity;
    int shadow_map;
    char _m0_final_padding[8];
};

struct SpotLight
{
    float3 position_ws;
    float3 direction_ws;
    float3 position_vs;
    float3 direction_vs;
    float4 color;
    float spotlight_half_angle;
    float range;
    float intensity;
    int shadow_map;
};

struct ShadowMap2DData
{
    float4x4 shadow_map_view_proj;
    float3 shadow_map_light_dir;
};

struct ShadowMapCubeData
{
    float cube_map_projection_near_z;
    float cube_map_projection_far_z;
    char _m0_final_padding[8];
};

struct PerViewData
{
    float4x4 view;
    float4x4 view_proj;
    float4 ambient_light;
    uint point_light_count;
    uint directional_light_count;
    uint spot_light_count;
    PointLight point_lights[16];
    DirectionalLight directional_lights[16];
    SpotLight spot_lights[16];
    ShadowMap2DData shadow_map_2d_data[32];
    ShadowMapCubeData shadow_map_cube_data[16];
};

struct PerObjectData
{
    float4x4 model;
};

struct MaterialData
{
    float4 base_color_factor;
    packed_float3 emissive_factor;
    float metallic_factor;
    float roughness_factor;
    float normal_texture_scale;
    float occlusion_texture_strength;
    float alpha_cutoff;
    uint has_base_color_texture;
    uint has_metallic_roughness_texture;
    uint has_normal_texture;
    uint has_occlusion_texture;
    uint has_emissive_texture;
};

struct MaterialDataUbo
{
    MaterialData data;
};

struct spvDescriptorSetBuffer0
{
    constant PerViewData* per_view_data [[id(0)]];
    array<texture2d<float>, 32> shadow_map_images [[id(3)]];
    array<texturecube<float>, 16> shadow_map_images_cube [[id(35)]];
};

struct spvDescriptorSetBuffer1
{
    constant MaterialDataUbo* per_material_data [[id(0)]];
    texture2d<float> base_color_texture [[id(1)]];
    texture2d<float> metallic_roughness_texture [[id(2)]];
    texture2d<float> normal_texture [[id(3)]];
    texture2d<float> occlusion_texture [[id(4)]];
    texture2d<float> emissive_texture [[id(5)]];
};

struct spvDescriptorSetBuffer2
{
    constant PerObjectData* per_object_data [[id(0)]];
};

struct main0_out
{
    float3 out_position_vs [[user(locn0)]];
    float3 out_normal_vs [[user(locn1)]];
    float3 out_tangent_vs [[user(locn2)]];
    float3 out_binormal_vs [[user(locn3)]];
    float2 out_uv [[user(locn4)]];
    float4 out_position_ws [[user(locn5)]];
    float3 out_model_view_0 [[user(locn6)]];
    float3 out_model_view_1 [[user(locn7)]];
    float3 out_model_view_2 [[user(locn8)]];
    float4 gl_Position [[position]];
};

struct main0_in
{
    float3 in_pos [[attribute(0)]];
    float3 in_normal [[attribute(1)]];
    float4 in_tangent [[attribute(2)]];
    float2 in_uv [[attribute(3)]];
};

vertex main0_out main0(main0_in in [[stage_in]], constant spvDescriptorSetBuffer0& spvDescriptorSet0 [[buffer(0)]], constant spvDescriptorSetBuffer1& spvDescriptorSet1 [[buffer(1)]], constant spvDescriptorSetBuffer2& spvDescriptorSet2 [[buffer(2)]])
{
    constexpr sampler smp(filter::linear, mip_filter::linear, address::repeat, compare_func::never, max_anisotropy(16));
    constexpr sampler smp_depth(filter::linear, mip_filter::linear, compare_func::greater, max_anisotropy(16));
    main0_out out = {};
    float3x3 out_model_view = {};
    float4x4 model_view_proj = (*spvDescriptorSet0.per_view_data).view_proj * (*spvDescriptorSet2.per_object_data).model;
    float4x4 model_view = (*spvDescriptorSet0.per_view_data).view * (*spvDescriptorSet2.per_object_data).model;
    out.gl_Position = model_view_proj * float4(in.in_pos, 1.0);
    out.out_position_vs = (model_view * float4(in.in_pos, 1.0)).xyz;
    out.out_normal_vs = float3x3(model_view[0].xyz, model_view[1].xyz, model_view[2].xyz) * in.in_normal;
    out.out_tangent_vs = float3x3(model_view[0].xyz, model_view[1].xyz, model_view[2].xyz) * in.in_tangent.xyz;
    float3 binormal = cross(in.in_normal, in.in_tangent.xyz) * in.in_tangent.w;
    out.out_binormal_vs = float3x3(model_view[0].xyz, model_view[1].xyz, model_view[2].xyz) * binormal;
    out.out_uv = in.in_uv;
    out.out_position_ws = (*spvDescriptorSet2.per_object_data).model * float4(in.in_pos, 1.0);
    out_model_view = float3x3(model_view[0].xyz, model_view[1].xyz, model_view[2].xyz);
    out.out_model_view_0 = out_model_view[0];
    out.out_model_view_1 = out_model_view[1];
    out.out_model_view_2 = out_model_view[2];
    return out;
}

