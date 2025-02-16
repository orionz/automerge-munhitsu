use crate::op_set;
use crate::op_set::OpSet;
use crate::types::{ListEncoding, ObjId};
use crate::{exid::ExId, Prop};

/// An iterator over the "parents" of an object
///
/// The "parent" of an object in this context is the ([`ExId`], [`Prop`]) pair which specifies the
/// location of this object in the composite object which contains it. Each element in the iterator
/// is a [`Parent`], yielded in reverse order. This means that once the iterator returns `None` you
/// have reached the root of the document.
///
/// This is returned by [`crate::ReadDoc::parents`]
#[derive(Debug)]
pub struct Parents<'a> {
    pub(crate) obj: ObjId,
    pub(crate) ops: &'a OpSet,
}

impl<'a> Parents<'a> {
    /// Return the path this `Parents` represents
    ///
    /// This is _not_ in reverse order.
    pub fn path(self) -> Vec<(ExId, Prop)> {
        let mut path = self
            .map(|Parent { obj, prop, .. }| (obj, prop))
            .collect::<Vec<_>>();
        path.reverse();
        path
    }

    /// Like `path` but returns `None` if the target is not visible
    pub fn visible_path(self) -> Option<Vec<(ExId, Prop)>> {
        let mut path = Vec::new();
        for Parent { obj, prop, visible } in self {
            if !visible {
                return None;
            }
            path.push((obj, prop))
        }
        path.reverse();
        Some(path)
    }
}

impl<'a> Iterator for Parents<'a> {
    type Item = Parent;

    fn next(&mut self) -> Option<Self::Item> {
        if self.obj.is_root() {
            None
        } else if let Some(op_set::Parent { obj, key, visible }) = self.ops.parent_object(&self.obj)
        {
            self.obj = obj;
            Some(Parent {
                obj: self.ops.id_to_exid(self.obj.0),
                prop: self
                    .ops
                    .export_key(self.obj, key, ListEncoding::List)
                    .unwrap(),
                visible,
            })
        } else {
            None
        }
    }
}

/// A component of a path to an object
#[derive(Debug, PartialEq, Eq)]
pub struct Parent {
    /// The object ID this component refers to
    pub obj: ExId,
    /// The property within `obj` this component refers to
    pub prop: Prop,
    /// Whether this component is "visible"
    ///
    /// An "invisible" component is one where the property is hidden, either because it has been
    /// deleted or because there is a conflict on this (object, property) pair and this value does
    /// not win the conflict.
    pub visible: bool,
}

#[cfg(test)]
mod tests {
    use super::Parent;
    use crate::{transaction::Transactable, Prop, ReadDoc};

    #[test]
    fn test_invisible_parents() {
        // Create a document with a list of objects, then delete one of the objects, then generate
        // a path to the deleted object.

        let mut doc = crate::AutoCommit::new();
        let list = doc
            .put_object(crate::ROOT, "list", crate::ObjType::List)
            .unwrap();
        let obj1 = doc.insert_object(&list, 0, crate::ObjType::Map).unwrap();
        let _obj2 = doc.insert_object(&list, 1, crate::ObjType::Map).unwrap();
        doc.put(&obj1, "key", "value").unwrap();
        doc.delete(&list, 0).unwrap();

        let mut parents = doc.parents(&obj1).unwrap().collect::<Vec<_>>();
        parents.reverse();
        assert_eq!(
            parents,
            vec![
                Parent {
                    obj: crate::ROOT,
                    prop: Prop::Map("list".to_string()),
                    visible: true,
                },
                Parent {
                    obj: list,
                    prop: Prop::Seq(0),
                    visible: false,
                },
            ]
        );
    }
}
