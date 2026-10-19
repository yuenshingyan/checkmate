pub fn original(n: u8) -> u8 {
    struct Counter {
        n: u8,
    }
    impl Counter {
        fn new(n: u8) -> Self {
            Self { n }
        }
        fn increment(&mut self) {
            self.n += 1;
        }
        fn finish(mut self) -> u8 {
            Self::increment(&mut self);
            self.n
        }
    }
    let counter = Counter::new(n);
    counter.finish()
}
