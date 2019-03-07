//! Various error wrappers
use luminance::framebuffer::FramebufferError;
use luminance::shader::program::ProgramError;

#[derive(Debug)]
pub enum LuminanceError {
    FramebufferError(FramebufferError),
    ProgramError(ProgramError),
}

impl From<FramebufferError> for LuminanceError {
    fn from(o: FramebufferError) -> Self {
        LuminanceError::FramebufferError(o)
    }
}

impl From<ProgramError> for LuminanceError {
    fn from(o: ProgramError) -> Self {
        LuminanceError::ProgramError(o)
    }
}
