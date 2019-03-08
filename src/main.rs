#[macro_use]
extern crate luminance;

#[macro_use]
extern crate luminance_derive;

use luminance::context::GraphicsContext;
use luminance::face_culling::{FaceCulling, FaceCullingMode, FaceCullingOrder};
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::BoundTexture;
use luminance::pixel::{Depth32F, Floating, R11G11B10F, RGB32F};
use luminance::render_state::RenderState;
use luminance::shader::program::Program;
use luminance::tess::{Mode, TessBuilder};
use luminance::texture::{Dim2, Dimensionable, Flat};
use luminance_glfw::event::{Action, Key, WindowEvent};
use luminance_glfw::surface::{GlfwSurface, Surface, WindowDim, WindowOpt};

use rand::distributions::{Distribution, Uniform};

use cgmath::prelude::*;
use cgmath::Matrix4;

mod error;
mod full_screen_tri;
mod passes;

#[derive(Copy, Clone, Debug, Eq, PartialEq, VertexAttribSem)]
pub enum Vertex3DColoredSemantics {
    #[sem(name = "pos", repr = "[f32; 3]", type_name = "Vertex3DPosition")]
    Position,
    #[sem(name = "color", repr = "[f32; 4]", type_name = "VertexColor")]
    Color,
}

#[derive(Vertex)]
#[vertex(sem = "Vertex3DColoredSemantics")]
struct Vertex3DColored {
    position: Vertex3DPosition,
    color: VertexColor,
}

const SIMPLE_FS: &'static str = include_str!("fs.glsl");
const SIMPLE_VS: &'static str = include_str!("vs.glsl");

const BLUR_SIZE_FACTOR: u32 = 4;

struct RenderBuffers {
    back_buffer: Framebuffer<Flat, Dim2, (), ()>,
    intermediate_buffer: Framebuffer<Flat, Dim2, (R11G11B10F, R11G11B10F), Depth32F>,
}

impl RenderBuffers {
    fn new<C: GraphicsContext>(c: &mut C, d: <Dim2 as Dimensionable>::Size) -> Self {
        Self {
            back_buffer: Framebuffer::back_buffer(d),
            intermediate_buffer: Framebuffer::new(c, d, 0).expect("intermediate framebuffer"),
        }
    }
}

luminance::uniform_interface! {
    struct GeometryShadeInterface {
        transform: [[f32; 4]; 4]
    }
}

luminance::uniform_interface! {
    struct FinalShadeInterface {
        main_tex: &'static BoundTexture<'static, Flat, Dim2, Floating>,
        bright_tex: &'static BoundTexture<'static, Flat, Dim2, Floating>
    }
}

#[inline]
fn rand_color(max_rgb: f32, alpha: f32) -> [f32; 4] {
    let distribution = Uniform::new_inclusive(0.0, max_rgb);
    let mut rng = rand::thread_rng();

    [
        distribution.sample(&mut rng),
        distribution.sample(&mut rng),
        distribution.sample(&mut rng),
        alpha,
    ]
}

fn gen_geometry() -> (Vec<Vertex3DColored>, Vec<u32>) {
    struct DedupKey {
        position: [f32; 3],
    }

    impl DedupKey {
        fn quantized_pos(&self) -> [i32; 3] {
            [
                (self.position[0] * 4096.0) as i32,
                (self.position[1] * 4096.0) as i32,
                (self.position[2] * 4096.0) as i32,
            ]
        }
    }

    impl PartialEq for DedupKey {
        fn eq(&self, o: &DedupKey) -> bool {
            self.quantized_pos() == o.quantized_pos()
        }
    }

    impl Eq for DedupKey {}

    impl std::hash::Hash for DedupKey {
        fn hash<H>(&self, state: &mut H)
        where
            H: std::hash::Hasher,
        {
            use std::hash::Hash;
            Hash::hash(&self.quantized_pos(), state);
        }
    }

    let (gltf, buffers, _) =
        gltf::import("res/sphere_cluster.glb").expect("Failed to  read icosphere data");

    let mut verts = Vec::new();
    let mut index_map = std::collections::HashMap::new();
    let mut indicies = Vec::new();

    assert!(gltf.meshes().len() == 1);
    let mesh = gltf.meshes().next().unwrap();

    for primitive in mesh.primitives() {
        use gltf::mesh::Semantic;
        assert!(primitive.mode() == gltf::mesh::Mode::Triangles);

        let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
        if let Some(iter) = reader.read_positions() {
            for mut vertex_position in iter {
                let index = index_map
                    .entry(DedupKey {
                        position: vertex_position,
                    })
                    .or_insert_with(|| {
                        let tr = verts.len() as u32;
                        verts.push(Vertex3DColored {
                            position: Vertex3DPosition::new(vertex_position),
                            color: VertexColor::new(rand_color(1.1, 1.0)),
                        });

                        tr
                    });
                indicies.push(*index);
            }
        }
    }
    assert!(indicies.len() % 3 == 0);

    (verts, indicies)
}

fn compute_rectilinearize_matrix(width: f32, height: f32) -> Matrix4<f32> {
    if width > height {
        Matrix4::from_nonuniform_scale(height / width, 1.0, 1.0)
    } else {
        Matrix4::from_nonuniform_scale(1.0, width / height, 1.0)
    }
}

fn main() {
    let mut surface = GlfwSurface::new(
        WindowDim::Windowed(1280, 720),
        "Hello, world!",
        WindowOpt::default().hide_cursor(true),
    )
    .expect("window creation");
    let (final_composite, _) = Program::<(), (), FinalShadeInterface>::from_strings(
        None,
        full_screen_tri::VS,
        None,
        include_str!("bloom.glsl"),
    )
    .expect("full screen shade creation");

    let (simple_prog, _) = Program::<Vertex3DColored, (), GeometryShadeInterface>::from_strings(
        None, SIMPLE_VS, None, SIMPLE_FS,
    )
    .expect("simple program creation");

    let fullscreen_triangles = TessBuilder::new(&mut surface)
        .set_vertex_nb(6)
        .set_mode(Mode::Triangle)
        .build()
        .expect("Fullscreen tris");

    let mut rectanglize = {
        let size = surface.size();
        compute_rectilinearize_matrix(size[1] as f32, size[0] as f32)
    };
    let mut aspect: f32 = surface.size()[1] as f32 / surface.size()[0] as f32;

    let (mut geometry_buffers, vertex_count) = {
        let (geometry, indices) = gen_geometry();
        (
            [
                TessBuilder::new(&mut surface)
                    .add_vertices(&geometry)
                    .set_indices(&indices)
                    .set_mode(Mode::Triangle)
                    .build()
                    .expect("geometry 0"),
                TessBuilder::new(&mut surface)
                    .add_vertices(&geometry)
                    .set_indices(&indices)
                    .set_mode(Mode::Triangle)
                    .build()
                    .expect("geometry 1"),
                TessBuilder::new(&mut surface)
                    .add_vertices(&geometry)
                    .set_indices(&indices)
                    .set_mode(Mode::Triangle)
                    .build()
                    .expect("geometry 2"),
            ],
            geometry.len(),
        )
    };

    let mut buffers = {
        let size = surface.size();
        RenderBuffers::new(&mut surface, size)
    };
    let mut blur_pass = {
        let size = surface.size();
        passes::BlurPass::new(
            &mut surface,
            [size[0] / BLUR_SIZE_FACTOR, size[1] / BLUR_SIZE_FACTOR],
            &fullscreen_triangles,
            0.25,
        )
        .expect("Blur pass creation")
    };
    let mut resize_size = None;
    let mut frame = 0;

    let mut colors: Vec<[f32; 4]> = (0..vertex_count).map(|_| rand_color(1.1, 1.0)).collect();

    let mut look_angles = (90.0, 0.0);
    let mut position = cgmath::Vector3::new(0.0, 0.0, 0.0);
    // Keys: WSADQE
    let mut key_states = [false, false, false, false, false, false];
    let mut control_active = true;

    'app: loop {
        for event in surface.poll_events() {
            match event {
                WindowEvent::Close | WindowEvent::Key(Key::Escape, _, Action::Release, _) => {
                    break 'app;
                }
                WindowEvent::Key(Key::W, _, action, _) => {
                    key_states[0] = action != Action::Release;
                }
                WindowEvent::Key(Key::S, _, action, _) => {
                    key_states[1] = action != Action::Release;
                }
                WindowEvent::Key(Key::A, _, action, _) => {
                    key_states[2] = action != Action::Release;
                }
                WindowEvent::Key(Key::D, _, action, _) => {
                    key_states[3] = action != Action::Release;
                }
                WindowEvent::Key(Key::Q, _, action, _) => {
                    key_states[4] = action != Action::Release;
                }
                WindowEvent::Key(Key::E, _, action, _) => {
                    key_states[5] = action != Action::Release;
                }
                WindowEvent::Key(Key::Space, _, Action::Press, _) => {
                    control_active = !control_active;
                    if control_active {
                        eprintln!("Controls enabled");
                    } else {
                        eprintln!("Controls disabled");
                    }
                }
                WindowEvent::Key(Key::P, _, Action::Press, _) => {
                    eprintln!("{:?} {:?}", position, look_angles);
                }
                WindowEvent::Key(Key::F1, _, Action::Press, _) => {
                    position = cgmath::Vector3::new(4.5, -4.5, 0.55);
                    look_angles = (94.6, -45.0);
                }
                WindowEvent::CursorPos(x, y) => {
                    if control_active {
                        look_angles.0 = 90.0 + y as f32 / 25.0;
                        look_angles.1 = x as f32 / -25.0;
                    }
                }
                WindowEvent::FramebufferSize(width, height) => {
                    resize_size = Some((width, height));
                }
                _ => (),
            }
        }

        const KEY_SPEED: f32 = 0.1;

        if control_active {
            use cgmath::{Deg, Matrix3, Vector3};
            let transform = Matrix3::from_angle_z(Deg(-look_angles.1))
                * Matrix3::new(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0)
                * Matrix3::from_angle_x(Deg(-look_angles.0));

            let up_vector = transform * Vector3::new(0.0, KEY_SPEED, 0.0);
            let right_vector = transform * Vector3::new(KEY_SPEED, 0.0, 0.0);
            let look_vector = transform * Vector3::new(0.0, 0.0, KEY_SPEED);
            if key_states[0] {
                position += look_vector;
            }
            if key_states[1] {
                position -= look_vector;
            }
            if key_states[2] {
                position += right_vector;
            }
            if key_states[3] {
                position -= right_vector;
            }
            if key_states[4] {
                position += up_vector;
            }
            if key_states[5] {
                position -= up_vector;
            }
        }

        if let Some((width, height)) = resize_size {
            resize_size = None;
            buffers = RenderBuffers::new(&mut surface, [width as u32, height as u32]);
            blur_pass
                .resize_buffers(
                    &mut surface,
                    [
                        width as u32 / BLUR_SIZE_FACTOR,
                        height as u32 / BLUR_SIZE_FACTOR,
                    ],
                )
                .expect("Blur pass resize");
            rectanglize = compute_rectilinearize_matrix(width as f32, height as f32);
            aspect = width as f32 / height as f32;
        }

        let transform = cgmath::perspective(cgmath::Deg(75.0), aspect, 0.001, 1000.0)
            * Matrix4::from_angle_x(cgmath::Deg(look_angles.0))
            * Matrix4::from_nonuniform_scale(1.0, 1.0, -1.0)
            * Matrix4::from_angle_z(cgmath::Deg(look_angles.1))
            * Matrix4::from_translation(position)
            * Matrix4::from_angle_z(cgmath::Deg(frame as f32));

        let curr_geometry_buffer = frame % geometry_buffers.len();

        // Main render
        surface.pipeline_builder().pipeline(
            &buffers.intermediate_buffer,
            [0.0, 0.0, 0.0, 0.0],
            |_, shader_gate| {
                shader_gate.shade(&simple_prog, |render_gate, interface| {
                    interface.transform.update(transform.into());
                    render_gate.render(
                        RenderState::default().set_face_culling(FaceCulling::new(
                            FaceCullingOrder::CCW,
                            FaceCullingMode::Front,
                        )),
                        |tesselation_gate| {
                            tesselation_gate.render(
                                &mut surface,
                                (&geometry_buffers[curr_geometry_buffer]).into(),
                            );
                        },
                    )
                })
            },
        );

        {
            // Update the geometry by tweaking color values
            let next_buffer_index =
                (curr_geometry_buffer + geometry_buffers.len() - 1) % geometry_buffers.len();
            let mut next_buffer_data = geometry_buffers[next_buffer_index]
                .as_slice_mut::<Vertex3DColored>()
                .expect("Getting next buffer binding");

            for i in 0..next_buffer_data.len() {
                next_buffer_data[i].color.repr = colors[i];
                for j in 0..3 {
                    colors[i][j] = (colors[i][j] + 0.01) % 1.1;
                }
            }
        }

        // Blur the bright texture, first injecting the intermediate buffer
        // brightness texture, and then flipping between horizontal and vertical
        // blurs
        blur_pass.run(&mut surface, &buffers.intermediate_buffer.color_slot().1);

        // Final composite pass
        surface.pipeline_builder().pipeline(
            &buffers.back_buffer,
            [0.0, 0.0, 0.0, 0.0],
            |pipeline, shader_gate| {
                let main_tex = pipeline.bind_texture(&buffers.intermediate_buffer.color_slot().0);
                let bright_tex = pipeline.bind_texture(blur_pass.texture());

                shader_gate.shade(&final_composite, |render_gate, interface| {
                    interface.main_tex.update(&main_tex);
                    interface.bright_tex.update(&bright_tex);

                    // interface.tex
                    render_gate.render(RenderState::default(), |tesselation_gate| {
                        tesselation_gate.render(&mut surface, (&fullscreen_triangles).into());
                    })
                })
            },
        );

        surface.swap_buffers();
        frame = frame + 1;
    }
}
