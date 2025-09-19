use crate::{processor::processor::MediaProcessor, tasks::Media};

pub struct ImageProcessor;

impl MediaProcessor for ImageProcessor {
    fn process_media(
        &self,
        media: &Media,
        tx: std::sync::mpsc::Sender<super::processor::ProcessMessage>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
