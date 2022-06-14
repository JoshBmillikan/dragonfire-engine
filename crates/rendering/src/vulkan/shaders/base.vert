#version 450

layout (location=0) in vec3 position;
layout (location=1) in vec3 normal;

layout (set=0, binding=0) uniform ubo {
    mat4 view;
    mat4 projection;
} ubo_data;

layout (push_constant) uniform constants {
    mat4 model;
} push_constants;

layout(location = 0) out vec3 frag_color;


void main() {
    mat4 transform = ubo_data.projection * ubo_data.view * push_constants.model;
    gl_Position = transform * vec4(position, 1.0);
    frag_color = vec3(1.0,0.0,0.0);
}