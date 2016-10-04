pub mod integrator;
pub mod vector;
pub mod mem;
pub mod cpu;


#[no_mangle]
pub extern "C" fn arm_main(mapper : &mut ::mem::MemoryMapper) -> !{

    // map vector tables
    mapper.map(::mem::PhysicalAddress(0), vector::VECTORS_ADDR, 1);
    vector::build_vector_table();

    // now we can create a normal page table!
    // map the vectors, stack and kernel as normal memory and then map the devices as device memory
/*
    let pagetable : pagetable;

    pagetable.map(kernel_start_phy, kernel_start_virt, kernel_end_virt-kernel_start_virt)
    pagetable.map(0, 0x..., PAGE_SIZE)
    pagetable.map( get_phys_stack, getsp(), PAGE_SIZE, NORMAL)
    pagetable.map( mmio, ?, PAGE_SIZE)

    memoryProtection.setRegion(kernel_start_virt, kernel_start_virt+WHATEVER, NORMAL)
    memoryProtection.map( mmio, whatever, PAGE_SIZE, DEVICE)
*/

    ::rust_main();

    loop {}
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    loop {}
}


#[no_mangle]
pub unsafe fn __aeabi_unwind_cpp_pr0() -> ()
{
    loop {}
}

#[no_mangle]
pub unsafe fn __aeabi_unwind_cpp_pr1() -> ()
{
    loop {}
}