use std::cell::Cell;
use std::collections::HashMap;

use super::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcHandle(pub(crate) u32);

impl GcHandle {
    #[allow(dead_code)]
    pub const NULL: GcHandle = GcHandle(u32::MAX);

    pub fn is_null(self) -> bool {
        self.0 == u32::MAX
    }
}

#[derive(Debug, Clone)]
pub enum GcData {
    List(Vec<Value>),
    Dict(Vec<(Value, Value)>),
    Set(Vec<Value>),
    StructFields(HashMap<String, Value>),
    ClassFields(HashMap<String, Value>),
    Captured(Vec<HashMap<String, Value>>),
}

impl GcData {
    fn collect_value_handles(v: &Value, out: &mut Vec<GcHandle>) {
        match v {
            Value::List(h) | Value::Set(h) | Value::Dict(h) => out.push(*h),
            Value::StructInstance { fields: h, .. } | Value::ClassInstance { fields: h, .. } => out.push(*h),
            Value::Closure { captured: h, .. } => out.push(*h),
            Value::Ok(inner) | Value::Err(inner) | Value::Some(inner) => Self::collect_value_handles(inner, out),
            Value::Iterator(iter) => super::value::trace_iter_kind_handles(iter, out),
            Value::TraitObject { value, methods, .. } => {
                Self::collect_value_handles(value, out);
                for (_, v) in methods {
                    Self::collect_value_handles(v, out);
                }
            }
            Value::Tuple(items) => {
                for item in items {
                    Self::collect_value_handles(item, out);
                }
            }
            _ => {}
        }
    }

    pub fn trace_children(&self, out: &mut Vec<GcHandle>) {
        match self {
            GcData::List(items) => {
                for item in items {
                    Self::collect_value_handles(item, out);
                }
            }
            GcData::Dict(pairs) => {
                for (k, v) in pairs {
                    Self::collect_value_handles(k, out);
                    Self::collect_value_handles(v, out);
                }
            }
            GcData::Set(items) => {
                for item in items {
                    Self::collect_value_handles(item, out);
                }
            }
            GcData::StructFields(fields) => {
                for (_, v) in fields {
                    Self::collect_value_handles(v, out);
                }
            }
            GcData::ClassFields(fields) => {
                for (_, v) in fields {
                    Self::collect_value_handles(v, out);
                }
            }
            GcData::Captured(scopes) => {
                for scope in scopes {
                    for (_, v) in scope {
                        Self::collect_value_handles(v, out);
                    }
                }
            }
        }
    }
}

thread_local! {
    pub static GC_HEAP: Cell<Option<*mut GcHeap>> = const { Cell::new(None) };
}

pub struct GcGuard;

impl GcGuard {
    pub fn set(heap: &mut GcHeap) -> Self {
        GC_HEAP.with(|h| h.set(Some(heap as *mut GcHeap)));
        GcGuard
    }
}

impl Drop for GcGuard {
    fn drop(&mut self) {
        GC_HEAP.with(|h| h.set(None));
    }
}

#[allow(dead_code)]
pub fn with_gc_mut<R>(heap: &mut GcHeap, f: impl FnOnce() -> R) -> R {
    let prev = GC_HEAP.with(|h| h.replace(Some(heap as *mut GcHeap)));
    let result = f();
    GC_HEAP.with(|h| h.set(prev));
    result
}

pub fn gc_heap() -> Option<&'static GcHeap> {
    GC_HEAP.with(|h| {
        h.get().map(|ptr| unsafe { &*ptr })
    })
}

pub fn gc_heap_mut() -> Option<&'static mut GcHeap> {
    GC_HEAP.with(|h| {
        h.get().map(|ptr| unsafe { &mut *ptr })
    })
}

struct GcSlot {
    marked: bool,
    data: GcData,
}

pub struct GcHeap {
    slots: Vec<Option<GcSlot>>,
    free: Vec<u32>,
}

impl GcHeap {
    pub fn new() -> Self {
        GcHeap {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn alloc(&mut self, data: GcData) -> GcHandle {
        let idx = if let Some(free_idx) = self.free.pop() {
            self.slots[free_idx as usize] = Some(GcSlot { marked: false, data });
            free_idx
        } else {
            let idx = self.slots.len() as u32;
            self.slots.push(Some(GcSlot { marked: false, data }));
            idx
        };
        GcHandle(idx)
    }

    pub fn get(&self, handle: GcHandle) -> &GcData {
        &self.slots[handle.0 as usize].as_ref().unwrap().data
    }

    pub fn get_mut(&mut self, handle: GcHandle) -> &mut GcData {
        &mut self.slots[handle.0 as usize].as_mut().unwrap().data
    }

    pub fn collect(&mut self, roots: &[GcHandle]) {
        let mut stack: Vec<u32> = Vec::new();
        for root in roots {
            if !root.is_null() {
                stack.push(root.0);
            }
        }

        while let Some(idx) = stack.pop() {
            let slot = match self.slots[idx as usize].as_mut() {
                Some(slot) => slot,
                None => continue,
            };
            if slot.marked {
                continue;
            }
            slot.marked = true;

            let mut children = Vec::new();
            slot.data.trace_children(&mut children);
            for child in children {
                if !child.is_null() {
                    let child_idx = child.0 as usize;
                    if child_idx < self.slots.len() {
                        if let Some(Some(child_slot)) = self.slots.get(child_idx) {
                            if !child_slot.marked {
                                stack.push(child.0);
                            }
                        }
                    }
                }
            }
        }

        for (i, slot) in self.slots.iter_mut().enumerate() {
            if let Some(s) = slot {
                if !s.marked {
                    *slot = None;
                    self.free.push(i as u32);
                } else {
                    s.marked = false;
                }
            }
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}
