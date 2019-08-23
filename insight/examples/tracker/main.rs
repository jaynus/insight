use insight::{Allocator, dump_alloc};

#[global_allocator]
static A: Allocator = Allocator;

fn main() -> Result<(), failure::Error> {
    println!("Hello World");

    // Get the stats?
    unsafe {
        dump_alloc();
    }

    //std::thread::sleep(std::time::Duration::from_secs(5));

    Ok(())
}