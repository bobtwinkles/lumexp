layout (location = 0) in vec2 pos;
layout (location = 1) in vec4 color;

out vec4 v_color;

void main() {
  gl_Position = vec4(pos, 0., 1.);
  v_color = color;
}
