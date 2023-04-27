use smol_str::SmolStr;
use std::fmt;
use std::fmt::Display;

use crate::types::OpId;
use crate::value::ScalarValue;
use crate::Automerge;
use std::borrow::Cow;

/// Marks let you store out-of-bound information about sequences.
///
/// The motivating use-case is rich text editing, see <https://www.inkandswitch.com/peritext/>.
/// Each position in the sequence can be affected by only one Mark of the same "name".
/// If multiple collaborators have set marks with the same name but different values
/// in overlapping ranges, automerge will chose a consistent (but arbitrary) value
/// when reading marks from the doc.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark<'a> {
    pub start: usize,
    pub end: usize,
    pub(crate) data: Cow<'a, MarkData>,
}

impl<'a> Mark<'a> {
    pub fn new<V: Into<ScalarValue>>(
        name: String,
        value: V,
        start: usize,
        end: usize,
    ) -> Mark<'static> {
        Mark {
            data: Cow::Owned(MarkData {
                name: name.into(),
                value: value.into(),
            }),
            start,
            end,
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        self.data.value.is_null()
    }

    pub(crate) fn from_data(start: usize, end: usize, data: &MarkData) -> Mark<'_> {
        Mark {
            data: Cow::Borrowed(data),
            start,
            end,
        }
    }

    pub fn into_owned(self) -> Mark<'static> {
        Mark {
            data: Cow::Owned(self.data.into_owned()),
            start: self.start,
            end: self.end,
        }
    }

    pub fn name(&self) -> &str {
        self.data.name.as_str()
    }

    pub fn value(&self) -> &ScalarValue {
        &self.data.value
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct MarkStateMachine<'a> {
    state: Vec<(OpId, Mark<'a>)>,
}

impl<'a> MarkStateMachine<'a> {
    pub(crate) fn mark_begin(
        &mut self,
        id: OpId,
        pos: usize,
        data: &'a MarkData,
        doc: &'a Automerge,
    ) -> Option<Mark<'a>> {
        self.mark_or_unmark_begin(id, pos, data, doc).and_then(|m| {
            if m.is_null() {
                None
            } else {
                Some(m)
            }
        })
    }

    pub(crate) fn mark_or_unmark_begin(
        &mut self,
        id: OpId,
        pos: usize,
        data: &'a MarkData,
        doc: &'a Automerge,
    ) -> Option<Mark<'a>> {
        let mut result = None;
        let index = self.find(id, doc).err()?;

        let mut mark = Mark::from_data(pos, pos, data);

        if let Some(above) = Self::mark_above(&self.state, index, &mark) {
            if above.value() == mark.value() {
                mark.start = above.start;
            }
        } else if let Some(below) = Self::mark_below(&mut self.state, index, &mark) {
            if below.value() == mark.value() {
                mark.start = below.start;
            } else {
                let mut m = below.clone();
                m.end = pos;
                result = Some(m);
            }
        }

        self.state.insert(index, (id, mark));

        result
    }

    pub(crate) fn mark_end(
        &mut self,
        id: OpId,
        pos: usize,
        doc: &'a Automerge,
    ) -> Option<Mark<'a>> {
        self.mark_or_unmark_end(id, pos, doc)
            .and_then(|m| if m.is_null() { None } else { Some(m) })
    }

    pub(crate) fn mark_or_unmark_end(
        &mut self,
        id: OpId,
        pos: usize,
        doc: &'a Automerge,
    ) -> Option<Mark<'a>> {
        let mut result = None;
        let index = self.find(id.prev(), doc).ok()?;

        let mut mark = self.state.remove(index).1;
        mark.end = pos;

        if Self::mark_above(&self.state, index, &mark).is_none() {
            match Self::mark_below(&mut self.state, index, &mark) {
                Some(below) if below.value() == mark.value() => {}
                Some(below) => {
                    below.start = pos;
                    result = Some(mark.clone());
                }
                None => {
                    result = Some(mark.clone());
                }
            }
        }

        result
    }

    fn find(&self, target: OpId, doc: &Automerge) -> Result<usize, usize> {
        let metadata = &doc.ops().m;
        self.state
            .binary_search_by(|probe| metadata.lamport_cmp(probe.0, target))
    }

    fn mark_above<'b>(
        state: &'b [(OpId, Mark<'a>)],
        index: usize,
        mark: &Mark<'a>,
    ) -> Option<&'b Mark<'a>> {
        Some(
            &state[index..]
                .iter()
                .find(|(_, m)| m.name() == mark.name())?
                .1,
        )
    }

    fn mark_below<'b>(
        state: &'b mut [(OpId, Mark<'a>)],
        index: usize,
        mark: &Mark<'a>,
    ) -> Option<&'b mut Mark<'a>> {
        Some(
            &mut state[0..index]
                .iter_mut()
                .filter(|(_, m)| m.data.name == mark.data.name)
                .last()?
                .1,
        )
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct MarkData {
    pub name: SmolStr,
    pub value: ScalarValue,
}

impl Display for MarkData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name={} value={}", self.name, self.value)
    }
}

/// ExpandMark allows you to decide whether new text inserted at the start/end of your
/// mark should also inherit the mark.
/// See <https://www.inkandswitch.com/peritext/> for details and
/// suggestions of which value to use for which operations when building a rich text editor.
#[derive(PartialEq, Debug, Clone, Copy)]
pub enum ExpandMark {
    Before,
    After,
    Both,
    None,
}

impl Default for ExpandMark {
    fn default() -> Self {
        Self::After
    }
}

impl ExpandMark {
    pub fn from(before: bool, after: bool) -> Self {
        match (before, after) {
            (true, true) => Self::Both,
            (false, true) => Self::After,
            (true, false) => Self::Before,
            (false, false) => Self::None,
        }
    }
    pub fn before(&self) -> bool {
        matches!(self, Self::Before | Self::Both)
    }
    pub fn after(&self) -> bool {
        matches!(self, Self::After | Self::Both)
    }
}
