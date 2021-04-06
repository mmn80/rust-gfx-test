#version 450

layout(location=0) in vec2 v_tex_coords;
layout(location=1) in vec3 v_position;
layout(location=2) in vec3 v_light_position;
layout(location=3) in vec3 v_view_position;

layout(location=0) out vec4 f_color;

layout(set = 1, binding = 0) uniform Light {
    vec3 light_position;
    vec3 light_color;
};

void main() {
    vec4 object_color = vec4(1.0, 0.0, 0.0, 0.0);
    vec4 object_normal = vec4(0.0, 0.0, 1.0, 0.0);

    float ambient_strength = 0.1;
    vec3 ambient_color = light_color * ambient_strength;

    vec3 normal = normalize(object_normal.rgb * 2.0 - 1.0);
    vec3 light_dir = normalize(v_light_position - v_position);
    
    float diffuse_strength = max(dot(normal, light_dir), 0.0);
    vec3 diffuse_color = light_color * diffuse_strength;

    vec3 view_dir = normalize(v_view_position - v_position);
    vec3 half_dir = normalize(view_dir + light_dir);
    float specular_strength = pow(max(dot(normal, half_dir), 0.0), 32);
    vec3 specular_color = specular_strength * light_color;

    vec3 result = (ambient_color + diffuse_color + specular_color) * object_color.xyz;
    f_color = vec4(result, object_color.a);
}
