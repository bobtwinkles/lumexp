#[macro_use]
extern crate luminance;

#[macro_use]
extern crate luminance_derive;

use luminance::context::GraphicsContext;
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::BoundTexture;
use luminance::pixel::{Depth32F, Floating, R11G11B10F, RGB32F};
use luminance::render_state::RenderState;
use luminance::shader::program::Program;
use luminance::tess::{Mode, TessBuilder};
use luminance::texture::{Dim2, Dimensionable, Flat};
use luminance_glfw::event::{Action, Key, WindowEvent};
use luminance_glfw::surface::{GlfwSurface, Surface, WindowDim, WindowOpt};

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
    use rand::distributions::{Distribution, Uniform};

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
    const COUNT_X: usize = 4;
    const COUNT_Y: usize = 4;
    const COUNT_Z: usize = 4;
    const COUNT: usize = COUNT_X * COUNT_Y * COUNT_Z;
    const SCALE: f32 = 0.5 / (COUNT_X as f32);

    let mut geometry = Vec::with_capacity(8 * COUNT);
    let mut indices = Vec::with_capacity(24 * COUNT);

    for x in 0..COUNT_X {
        let nx = ((x as f32 + 0.5) / (COUNT_X as f32) * 2.0 - 1.0) / (2.0_f32.sqrt());
        for y in 0..COUNT_Y {
            let ny = ((y as f32 + 0.5) / (COUNT_Y as f32) * 2.0 - 1.0) / (2.0_f32.sqrt());
            for z in 0..COUNT_Z {
                let nz = ((z as f32 + 0.5) / (COUNT_Z as f32) * 2.0 - 1.0) / (2.0_f32.sqrt());

                let colors: Vec<[f32; 4]> = (0..8).map(|_| rand_color(1.1, 1.0)).collect();

                for i in 0..8 {
                    let x = if i & 1 != 0 { nx + SCALE } else { nx - SCALE };
                    let y = if i & 2 != 0 { ny + SCALE } else { ny - SCALE };
                    let z = if i & 4 != 0 { nz + SCALE } else { nz - SCALE };
                    geometry.push(Vertex3DColored {
                        position: Vertex3DPosition::new([x, y, z]),
                        color: VertexColor::new(colors[i]),
                    })
                }

                let index_base = (8 * ((z * COUNT_X * COUNT_Y) + (y * COUNT_X) + x)) as u32;
                //        +-----+
                //        |110  |111
                //        |  0  |
                //  ------+-----+-----+-----+
                //  |110  |100  |101  |111  |110
                //  |  1  |  2  |  3  |  4  |
                //  ------+-----+-----+-----+
                //        |000  |001   011   010
                //        |  5  |
                //        +-----+
                //         010   011
                // Faces 0, 2, and 3 are CCW
                indices.push(index_base + 0b100); // Face 0
                indices.push(index_base + 0b101);
                indices.push(index_base + 0b110);
                indices.push(index_base + 0b101);
                indices.push(index_base + 0b110);
                indices.push(index_base + 0b111);

                indices.push(index_base + 0b010); // Face 1
                indices.push(index_base + 0b110);
                indices.push(index_base + 0b000);
                indices.push(index_base + 0b110);
                indices.push(index_base + 0b100);
                indices.push(index_base + 0b000);

                indices.push(index_base + 0b100); // Face 2
                indices.push(index_base + 0b000);
                indices.push(index_base + 0b001);
                indices.push(index_base + 0b001);
                indices.push(index_base + 0b101);
                indices.push(index_base + 0b100);

                indices.push(index_base + 0b101); // Face 3
                indices.push(index_base + 0b001);
                indices.push(index_base + 0b011);
                indices.push(index_base + 0b101);
                indices.push(index_base + 0b011);
                indices.push(index_base + 0b111);

                indices.push(index_base + 0b010); // Face 4
                indices.push(index_base + 0b011);
                indices.push(index_base + 0b111);
                indices.push(index_base + 0b111);
                indices.push(index_base + 0b110);
                indices.push(index_base + 0b010);

                indices.push(index_base + 0b000); // Face 5
                indices.push(index_base + 0b001);
                indices.push(index_base + 0b011);
                indices.push(index_base + 0b011);
                indices.push(index_base + 0b010);
                indices.push(index_base + 0b000);
            }
        }
    }

    (geometry, indices)
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
        WindowOpt::default(),
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

    let mut transform = Matrix4::<f32>::identity();
    let mut rectanglize = {
        let size = surface.size();
        compute_rectilinearize_matrix(size[1] as f32, size[0] as f32)
    };

    let (geometry, indices) = gen_geometry();
    let geometry_triangles = TessBuilder::new(&mut surface)
        .add_vertices(&geometry)
        .set_indices(&indices)
        .set_mode(Mode::Triangle)
        .build()
        .expect("Triangle geometry");

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

    'app: loop {
        for event in surface.poll_events() {
            match event {
                WindowEvent::Close | WindowEvent::Key(Key::Escape, _, Action::Release, _) => {
                    break 'app;
                }
                WindowEvent::FramebufferSize(width, height) => {
                    resize_size = Some((width, height));
                }
                _ => (),
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
        }

        transform = rectanglize
            * Matrix4::from_angle_z(cgmath::Deg(1.0 * frame as f32))
            * Matrix4::from_angle_x(cgmath::Deg(1.0 * 0.5 * frame as f32))
            * Matrix4::from_angle_y(cgmath::Deg(1.0 * 0.25 * frame as f32));

        // Main render
        surface.pipeline_builder().pipeline(
            &buffers.intermediate_buffer,
            [0.0, 0.0, 0.0, 0.0],
            |_, shader_gate| {
                shader_gate.shade(&simple_prog, |render_gate, interface| {
                    interface.transform.update(transform.into());
                    render_gate.render(RenderState::default(), |tesselation_gate| {
                        tesselation_gate.render(&mut surface, (&geometry_triangles).into());
                    })
                })
            },
        );

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
