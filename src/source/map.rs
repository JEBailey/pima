use std::sync::Arc;

use super::SourceId;

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub id: SourceId,
    pub name: Arc<str>,
    pub text: Arc<str>,
}

#[derive(Default, Debug)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add(&mut self, name: impl Into<Arc<str>>, text: impl Into<Arc<str>>) -> SourceId {
        let id = SourceId(self.files.len() as u32);
        self.files.push(SourceFile {
            id,
            name: name.into(),
            text: text.into(),
        });
        id
    }

    pub fn get(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    /// Returns a one-based line and column for a byte offset.
    pub fn line_column(&self, id: SourceId, offset: usize) -> Option<(usize, usize)> {
        let source = self.get(id)?;
        if offset > source.text.len() || !source.text.is_char_boundary(offset) {
            return None;
        }
        let prefix = &source.text[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
        let column = source.text[line_start..offset].chars().count() + 1;
        Some((line, column))
    }
}
