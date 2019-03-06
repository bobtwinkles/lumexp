#[macro_use]
extern crate luminance;

use luminance::context::GraphicsContext;
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::BoundTexture;
use luminance::pixel::{Depth32F, RGB32F};
use luminance::render_state::RenderState;
use luminance::shader::program::Program;
use luminance::tess::{Mode, Tess};
use luminance::texture::{Dim2, Dimensionable, Flat, Texture};
use luminance_glfw::event::{Action, Key, WindowEvent};
use luminance_glfw::surface::{GlfwSurface, Surface, WindowDim, WindowOpt};

mod full_screen_tri;

type Vertex2DColored = ([f32; 2], [f32; 4]);

const SIMPLE_FS: &'static str = include_str!("fs.glsl");
const SIMPLE_VS: &'static str = include_str!("vs.glsl");

const GEOM_VERTS: [Vertex2DColored; 3] = [
    ([0.5, -0.5], [0., 1., 0., 1.0]),
    ([0.0, 0.5], [0., 0., 1., 1.0]),
    ([-0.5, -0.5], [1., 0., 0., 1.0]),
];

struct RenderBuffers {
    back_buffer: Framebuffer<Flat, Dim2, (), ()>,
    intermediate_buffer: Framebuffer<
        Flat,
        Dim2,
        (Texture<Flat, Dim2, RGB32F>, Texture<Flat, Dim2, RGB32F>),
        Texture<Flat, Dim2, Depth32F>,
    >,
    blur_buffer0: Framebuffer<Flat, Dim2, Texture<Flat, Dim2, RGB32F>, ()>,
    blur_buffer1: Framebuffer<Flat, Dim2, Texture<Flat, Dim2, RGB32F>, ()>,
}

impl RenderBuffers {
    fn new<C: GraphicsContext>(c: &mut C, d: <Dim2 as Dimensionable>::Size) -> Self {
        let half_d = [d[0] / 2, d[1] / 2];
        Self {
            back_buffer: Framebuffer::back_buffer(d),
            intermediate_buffer: Framebuffer::new(c, d, 0).expect("intermediate framebuffer"),
            blur_buffer0: Framebuffer::new(c, half_d, 0).expect("blur framebuffer 0"),
            blur_buffer1: Framebuffer::new(c, half_d, 0).expect("blur framebuffer 1"),
        }
    }
}

luminance::uniform_interface! {
    struct FinalShadeInterface {
        main_tex: &'static BoundTexture<'static, Flat, Dim2, RGB32F>,
        bright_tex: &'static BoundTexture<'static, Flat, Dim2, RGB32F>
    }
}

luminance::uniform_interface! {
    struct BlurInterface {
        horizontal: bool,
        blur_tex: &'static BoundTexture<'static, Flat, Dim2, RGB32F>
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

    let (blur_prog, _) = Program::<(), (), BlurInterface>::from_strings(
        None,
        full_screen_tri::VS,
        None,
        include_str!("blur.glsl"),
    ).expect("blur shader");

    let (simple_prog, _) =
        Program::<Vertex2DColored, (), ()>::from_strings(None, SIMPLE_VS, None, SIMPLE_FS)
            .expect("simple program creation");

    let fullscreen_triangles = Tess::attributeless(&mut surface, Mode::Triangle, 6);
    let geometry_triangles = Tess::new(&mut surface, Mode::Triangle, &GEOM_VERTS[..], None);

    let mut buffers = {
        let size = surface.size();
        RenderBuffers::new(&mut surface, size)
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
        surface.pipeline_builder().pipeline(
            &buffers.blur_buffer0,
            [0.0, 0.0, 0.0, 0.0],
            |pipeline, shader_gate| {
                let tex = pipeline.bind_texture(&buffers.intermediate_buffer.color_slot().1);

                shader_gate.shade(&blur_prog, |render_gate, interface| {
                    interface.horizontal.update(false);
                    interface
                        .blur_tex
                        .update(&tex);

                    render_gate.render(RenderState::default(), |tesselation_gate| {
                        tesselation_gate.render(&mut surface, (&fullscreen_triangles).into());
                    })
                })
            },
        );

        for _ in 0..5 {
            surface.pipeline_builder().pipeline(
                &buffers.blur_buffer1,
                [0.0, 0.0, 0.0, 0.0],
                |pipeline, shader_gate| {
                    let tex = pipeline.bind_texture(buffers.blur_buffer0.color_slot());

                    shader_gate.shade(&blur_prog, |render_gate, interface| {
                        interface.horizontal.update(true);
                        interface.blur_tex.update(&tex);

                        render_gate.render(RenderState::default(), |tesselation_gate| {
                            tesselation_gate.render(&mut surface, (&fullscreen_triangles).into());
                        })
                    })
                },
            );

            surface.pipeline_builder().pipeline(
                &buffers.blur_buffer0,
                [0.0, 0.0, 0.0, 0.0],
                |pipeline, shader_gate| {
                    let tex = pipeline.bind_texture(&buffers.blur_buffer1.color_slot());

                    shader_gate.shade(&blur_prog, |render_gate, interface| {
                        interface.horizontal.update(false);
                        interface
                            .blur_tex
                            .update(&tex);

                        render_gate.render(RenderState::default(), |tesselation_gate| {
                            tesselation_gate.render(&mut surface, (&fullscreen_triangles).into());
                        })
                    })
                },
            );
        }

        // Final composite pass
        surface.pipeline_builder().pipeline(
            &buffers.back_buffer,
            [0.0, 0.0, 0.0, 0.0],
            |pipeline, shader_gate| {
                let main_tex = pipeline.bind_texture(&buffers.intermediate_buffer.color_slot().0);
                let bright_tex = pipeline.bind_texture(&buffers.blur_buffer0.color_slot());

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
