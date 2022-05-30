
pub trait RenderingEngine {

}

pub fn create_rendering_engine<R: RenderingEngine>() -> Box<R> {
    todo!()
}
