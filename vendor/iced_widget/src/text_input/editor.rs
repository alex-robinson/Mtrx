use crate::text_input::{Cursor, Value, cursor};

pub struct Editor<'a> {
    value: &'a mut Value,
    cursor: &'a mut Cursor,
}

impl<'a> Editor<'a> {
    pub fn new(value: &'a mut Value, cursor: &'a mut Cursor) -> Editor<'a> {
        Editor { value, cursor }
    }

    pub fn contents(&self) -> String {
        self.value.to_string()
    }

    pub fn insert(&mut self, character: char) {
        if let Some((left, right)) = self.cursor.selection(self.value) {
            self.cursor.move_left(self.value);
            self.value.remove_many(left, right);
        }

        self.value.insert(self.cursor.end(self.value), character);
        self.cursor.move_right(self.value);
    }

    pub fn paste(&mut self, content: Value) {
        let length = content.len();
        if let Some((left, right)) = self.cursor.selection(self.value) {
            self.cursor.move_left(self.value);
            self.value.remove_many(left, right);
        }

        self.value.insert_many(self.cursor.end(self.value), content);

        self.cursor.move_right_by_amount(self.value, length);
    }

    pub fn backspace(&mut self) {
        match self.cursor.selection(self.value) {
            Some((start, end)) => {
                self.cursor.move_left(self.value);
                self.value.remove_many(start, end);
            }
            None => {
                let start = self.cursor.start(self.value);

                if start > 0 {
                    self.cursor.move_left(self.value);
                    self.value.remove(start - 1);
                }
            }
        }
    }

    // Mtrx patch (surrounding text): an input method's
    // `DeleteSurrounding`.
    /// Deletes `before` and `after` bytes around the selection (or the
    /// cursor), keeping the selection. A count ending inside a grapheme
    /// takes all of it.
    pub fn delete_surrounding(&mut self, before: usize, after: usize) {
        let state = self.cursor.state(self.value);
        let (start, end) =
            self.cursor.selection(self.value).unwrap_or_else(|| {
                (self.cursor.end(self.value), self.cursor.end(self.value))
            });

        let from = self.value.back_by_bytes(start, before);
        let to = self.value.forward_by_bytes(end, after);

        // After the selection first, so that what precedes it stays put.
        self.value.remove_many(end, to);
        self.value.remove_many(from, start);

        let shift = start - from;

        match state {
            cursor::State::Index(index) => self.cursor.move_to(index - shift),
            cursor::State::Selection { start, end } => {
                self.cursor.select_range(start - shift, end - shift);
            }
        }
    }

    pub fn delete(&mut self) {
        match self.cursor.selection(self.value) {
            Some(_) => {
                self.backspace();
            }
            None => {
                let end = self.cursor.end(self.value);

                if end < self.value.len() {
                    self.value.remove(end);
                }
            }
        }
    }
}
