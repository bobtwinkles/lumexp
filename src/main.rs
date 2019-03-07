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

mod error;
mod full_screen_tri;
mod passes;

#[derive(Copy, Clone, Debug, Eq, PartialEq, VertexAttribSem)]
pub enum Vertex2DColoredSemantics {
    #[sem(name = "pos", repr = "[f32; 2]", type_name = "Vertex2DPosition")]
    Position,
    #[sem(name = "color", repr = "[f32; 4]", type_name = "VertexColor")]
    Color,
}

#[derive(Vertex)]
#[vertex(sem = "Vertex2DColoredSemantics")]
struct Vertex2DColored {
    position: Vertex2DPosition,
    color: VertexColor,
}

const SIMPLE_FS: &'static str = include_str!("fs.glsl");
const SIMPLE_VS: &'static str = include_str!("vs.glsl");

const GEOM_VERTS: [Vertex2DColored; 3] = [
    Vertex2DColored {
        position: Vertex2DPosition::new([0.5, -0.5]),
        color: VertexColor::new([0., 1., 0., 1.0]),
    },
    Vertex2DColored {
        position: Vertex2DPosition::new([0.0, 0.5]),
        color: VertexColor::new([0., 0., 1., 1.0]),
    },
    Vertex2DColored {
        position: Vertex2DPosition::new([-0.5, -0.5]),
        color: VertexColor::new([1., 0., 0., 1.0]),
    },
];

const BLUR_SIZE_FACTOR: u32 = 4;

struct RenderBuffers {
    back_buffer: Framebuffer<Flat, Dim2, (), ()>,
    intermediate_buffer: Framebuffer<Flat, Dim2, (RGB32F, RGB32F), Depth32F>,
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
    struct FinalShadeInterface {
        main_tex: &'static BoundTexture<'static, Flat, Dim2, Floating>,
        bright_tex: &'static BoundTexture<'static, Flat, Dim2, Floating>
    }
}

fn main() {
    let mut surface = GlfwSurface::new(
        WindowDim::Windowed(512, 512),
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

    let (simple_prog, _) =
        Program::<Vertex2DColored, (), ()>::from_strings(None, SIMPLE_VS, None, SIMPLE_FS)
            .expect("simple program creation");

    let fullscreen_triangles = TessBuilder::new(&mut surface)
        .set_vertex_nb(6)
        .set_mode(Mode::Triangle)
        .build()
        .expect("Fullscreen tris");
    let geometry_triangles = TessBuilder::new(&mut surface)
        .add_vertices(&GEOM_VERTS[..])
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
                .expect("Blur pass resize")
        }

        // Main render
        surface.pipeline_builder().pipeline(
            &buffers.intermediate_buffer,
            [0.0, 0.0, 0.0, 0.0],
            |_, shader_gate| {
                shader_gate.shade(&simple_prog, |render_gate, _| {
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
    }
}
