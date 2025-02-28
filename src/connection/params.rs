use std::io::Result;

pub use pg_lambda_macros::query_params;

pub trait QueryParams {
    fn write(self, dst: &mut Vec<u8>) -> Result<()>;
}

pub struct NoParams;

impl QueryParams for NoParams {
    fn write(self, _: &mut Vec<u8>) -> Result<()> {
        Ok(())
    }
}

pub struct ParamsFn<F>(F);

impl<F> ParamsFn<F> {
    pub(crate) fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> QueryParams for ParamsFn<F>
where
    F: FnOnce(&mut Vec<u8>) -> Result<()>,
{
    fn write(self, dst: &mut Vec<u8>) -> Result<()> {
        (self.0)(dst)
    }
}
