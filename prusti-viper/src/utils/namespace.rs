#[derive(Debug, Clone)]
pub struct Namespace {
    prefix: String,
    counter: u64
}

impl Namespace {
    pub fn new(prefix: impl ToString) -> Self {
        let prefix = prefix.to_string();
        Namespace { prefix, counter: 0 }
    }

    pub fn next(&mut self) -> String {
        self.counter += 1;
        format!("{}${}", self.prefix, self.counter)
    }

    pub fn next_child(&mut self) -> Namespace {
        Namespace::new(self.next())
    }
}
