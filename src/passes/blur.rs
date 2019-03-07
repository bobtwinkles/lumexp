//! The blur pass
use luminance::context::GraphicsContext;
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::BoundTexture;
use luminance::pixel::{Floating, Pixel, R11G11B10F};
use luminance::render_state::RenderState;
use luminance::shader::program::Program;
use luminance::tess::Tess;
use luminance::texture::{Dim2, Flat, Texture};

use crate::error::LuminanceError;
use crate::Vertex2DColored;

luminance::uniform_interface! {
    struct BlurInterface {
        blur_tex: &'static BoundTexture<'static, Flat, Dim2, Floating>,
        radius: f32
    }
}

pub struct BlurPass<'a> {
    program: Program<Vertex2DColored, (), BlurInterface>,
    buffers: [Framebuffer<Flat, Dim2, R11G11B10F, ()>; 2],
    fullscreen_triangles: &'a Tess,
    radius_factor: f32,
}

impl<'a> BlurPass<'a> {
    /// Create a new blur pass with the provided backbuffer dimensions.
    pub fn new(
        c: &mut impl GraphicsContext,
        d: [u32; 2],
        fullscreen_tris: &'a Tess,
        radius_factor: f32,
    ) -> Result<Self, LuminanceError> {
        let (program, warnings) = Program::from_strings(
            None,
            crate::full_screen_tri::VS,
            None,
            include_str!("blur.glsl"),
        )?;
        if warnings.len() != 0 {
            eprintln!("Warnings during blur pass program compilation:");
            for warning in warnings {
                eprintln!(" {:?}", warning)
            }
        }
        Ok(Self {
            program: program,
            fullscreen_triangles: fullscreen_tris,
            buffers: [Framebuffer::new(c, d, 0)?, Framebuffer::new(c, d, 0)?],
            radius_factor,
        })
    }

    pub fn resize_buffers(
        &mut self,
        c: &mut impl GraphicsContext,
        d: [u32; 2],
    ) -> Result<(), LuminanceError> {
        self.buffers = [Framebuffer::new(c, d, 0)?, Framebuffer::new(c, d, 0)?];
        Ok(())
    }

    pub fn run<C, P>(&self, context: &mut C, texture: &Texture<Flat, Dim2, P>)
    where
        C: GraphicsContext,
        P: Pixel<SamplerType = Floating>,
    {
        let num_buffers = self.buffers.len();
        // Initial injection of new data
        context.pipeline_builder().pipeline(
            // Since we always read from buffer j + 1, and j starts at 0, we
            // want to put the initial data in buffer 1 regardless of the total
            // number of buffers
            &self.buffers[1],
            [0.0, 0.0, 0.0, 0.0],
            |pipeline, shader_gate| {
                let tex = pipeline.bind_texture(texture);

                shader_gate.shade(&self.program, |render_gate, interface| {
                    interface.radius.update(0.25);
                    interface.blur_tex.update(&tex);

                    render_gate.render(RenderState::default(), |tesselation_gate| {
                        tesselation_gate.render(context, (self.fullscreen_triangles).into());
                    })
                })
            },
        );

        for i in 0..2 {
            let rad: f32 = (num_buffers * i) as f32 * self.radius_factor + 0.25;
            // Blur through all the buffers, ending on the last one
            for j in 0..num_buffers {
                context.pipeline_builder().pipeline(
                    &self.buffers[j],
                    [0.0, 0.0, 0.0, 0.0],
                    |pipeline, shader_gate| {
                        let tex =
                            pipeline.bind_texture(self.buffers[(j + 1) % num_buffers].color_slot());

                        shader_gate.shade(&self.program, |render_gate, interface| {
                            interface.radius.update(rad as f32);
                            interface.blur_tex.update(&tex);

                            render_gate.render(RenderState::default(), |tesselation_gate| {
                                tesselation_gate
                                    .render(context, (self.fullscreen_triangles).into());
                            })
                        })
                    },
                );
            }
        }
    }

    /// Get the texture containing the output of the blur
    pub fn texture(&self) -> &Texture<Flat, Dim2, R11G11B10F> {
        let num_buffers = self.buffers.len();
        // Since the blur always ends by writing to the last buffer, return that one
        &self.buffers[num_buffers - 1].color_slot()
    }
}
