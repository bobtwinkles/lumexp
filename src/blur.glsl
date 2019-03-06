in vec2 v_pos;

uniform sampler2D blur_tex;
uniform bool horizontal;

out vec4 color;

const float weight[5] = float[](0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);

void main() {
  vec2 tex_offset = 1.0 / textureSize(blur_tex, 0);
  vec4 result = texture(blur_tex, v_pos) * weight[0];
  if (horizontal) {
    for (int i = 1; i < 5; ++i) {
      result += texture(blur_tex, v_pos + vec2(tex_offset.x * i, 0.0)) * weight[i];
      result += texture(blur_tex, v_pos - vec2(tex_offset.x * i, 0.0)) * weight[i];
    }
  } else {
    for (int i = 1; i < 5; ++i) {
      result += texture(blur_tex, v_pos + vec2(0.0, tex_offset.y * i)) * weight[i];
      result += texture(blur_tex, v_pos - vec2(0.0, tex_offset.y * i)) * weight[i];
    }
  }
  color = result;
}
