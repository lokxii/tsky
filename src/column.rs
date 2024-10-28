use crate::{thread_view::ThreadView, UpdatingFeed};

pub enum Column {
    UpdatingFeed(UpdatingFeed),
    Thread(ThreadView),
}

pub struct ColumnStack {
    pub stack: Vec<Column>,
}

impl ColumnStack {
    pub fn from(stack: Vec<Column>) -> ColumnStack {
        ColumnStack { stack }
    }

    pub fn push(&mut self, column: Column) {
        self.stack.push(column);
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn last(&self) -> Option<&Column> {
        self.stack.last()
    }

    pub fn last_mut(&mut self) -> Option<&mut Column> {
        self.stack.last_mut()
    }
}
