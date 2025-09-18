use crate::{processor::processor::MediaProcessor, tasks::Media};

pub struct VideoProcessor;

impl MediaProcessor for VideoProcessor {
    fn process_media<'m>(
        &self,
        media: &'m Media,
        tx: std::sync::mpsc::Sender<super::processor::ProcessMessage>,
    ) -> anyhow::Result<(), &'m Media> {
        Ok(())
    }
}
