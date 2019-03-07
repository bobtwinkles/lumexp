layout (location = 0) in vec3 pos;
layout (location = 1) in vec4 color;

uniform mat4 transform;

out vec4 v_color;

void main() {
  gl_Position = transform * vec4(pos, 1.);
  v_color = color;
}
