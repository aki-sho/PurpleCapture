use parking_lot::Mutex;
use std::process::Child;

#[derive(Default)]
pub struct ProcessManager {
    children: Mutex<Vec<Child>>,
}

impl ProcessManager {
    #[allow(dead_code)]
    pub fn track(&self, child: Child) {
        self.children.lock().push(child);
    }

    pub fn stop_all(&self) {
        let mut children = self.children.lock();
        for child in children.iter_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        children.clear();
    }
}
