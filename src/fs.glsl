in vec4 v_color;

layout(location = 0) out vec4 main_color;
layout(location = 1) out vec4 bright_color;

void main() {
  if (dot(v_color, vec4(0.2126, 0.7152, 0.0722, 0.0)) > 0.8) {
    bright_color = v_color;
  } else {
    bright_color = vec4(0.0);
  }
  main_color = v_color;
}
