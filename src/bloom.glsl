in vec2 v_pos;

uniform sampler2D main_tex;
uniform sampler2D bright_tex;

out vec4 color;

void main() {
  color = texture(bright_tex, v_pos) + texture(main_tex, v_pos);

  color = pow(color, vec4(1.0 / 2.2));
}
