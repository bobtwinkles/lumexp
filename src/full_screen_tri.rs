pub const VS: &'static str = r#"
out vec2 v_pos;

const vec2[6] POS = vec2[](
  vec2(0.0, 1.0),
  vec2(0.0, 0.0),
  vec2(1.0, 1.0),
  vec2(0.0, 0.0),
  vec2(1.0, 1.0),
  vec2(1.0, 0.0)
);

void main() {
  vec2 pos = POS[gl_VertexID];
  gl_Position = vec4((pos * 2.0) - vec2(1.0), 0.0, 1.0);
  v_pos = pos;
}
"#;
