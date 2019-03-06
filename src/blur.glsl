in vec2 v_pos;

uniform sampler2D blur_tex;
uniform float radius;

out vec4 color;

void main() {
  vec2 tex_offset = 0.3 * (radius + 1) / textureSize(blur_tex, 0);
  vec4 result = vec4(0.0);
  result += texture(blur_tex, v_pos + tex_offset);
  result += texture(blur_tex, v_pos - tex_offset);
  tex_offset.x = -tex_offset.x;
  result += texture(blur_tex, v_pos + tex_offset);
  result += texture(blur_tex, v_pos - tex_offset);
  color = result / 4.0;
}
