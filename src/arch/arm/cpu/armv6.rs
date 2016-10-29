

#[inline(always)]
pub fn data_memory_barrier() {
    unsafe {
        // http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.ddi0344k/I1001599.html
        asm!("mcr p15, 0, $0, c7, c10, 5"::"r"(0):"memory":"volatile")
    }
}

#[inline(always)]
pub fn data_synchronization_barrier() {
    unsafe {
        // http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.ddi0344k/I1001599.html
        asm!("mcr p15, 0, $0, c7, c10, 4"::"r"(0):"volatile")
    }
}

#[inline(never)]
#[naked]
pub fn wait_for_interrupts() {
    unsafe {
        asm!("loop:
            mcr p15, 0, $0, c7, c0, 4
            b loop
            "::"r"(0)::"volatile"
            )
    }
}