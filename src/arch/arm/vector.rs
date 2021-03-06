use core::slice;
use platform;

use collections::boxed::Box;
use core::borrow::Borrow;

// processor jumps to address 0 so must be ID mapper here for now, till (if?) if will relocate the vectors.
// pub const VECTORS_ADDR : ::mem::VirtualAddress  = ::mem::VirtualAddress(0xea00_0000);
pub const VECTORS_ADDR: ::mem::VirtualAddress = ::mem::VirtualAddress(0x0);


/* TODO: only use this macro for interrupts */
/* and not for data abort for example */
// NOTE: DO NOT change struct without changing the inline assembly in vector_entry
#[repr(C, packed)]
#[derive(Copy, Clone, Debug)]
pub struct InterruptContext {
    sp:   u32,
    lr:   u32,
    cpsr: u32,
    r0: u32,
    r1: u32,
    r2: u32,
    r3: u32,
    r4: u32,
    r5: u32,
    r6: u32,
    r7: u32,
    r8: u32,
    r9: u32,
    r10: u32,
    r11: u32,
    r12: u32,
    pc: u32,
}

// can't use sizeof... https://github.com/rust-lang/rfcs/issues/1144
const SIZE_OF_INT_CTX : usize = 4*(1+1+1+13+1);


macro_rules! inthandler {
    ( $handler:ident ) => {{ 

extern "C" fn vector_with_context(ctx : &InterruptContext) {

    // copy the interrupt context from interrupt stack to our stack
    // (we are on kernel stack)
    let mut c : InterruptContext = *ctx;
    
    $handler(&mut c);

    // restore everything - returns interrupted code.
    unsafe{
    asm!("mov r0, $0
        /* r0 has InterruptContext. sub 4 so 'pop' will work */
        sub r0, r0, 4
        /* pop things in reverse order than in vector_entry */
        /* seems that using sp in ldmia is deprecated so we use ldr...  http://infocenter.arm.com/help/index.jsp?topic=/com.arm.doc.dui0204j/Cihcadda.html */
        ldr sp, [r0, #4]!
        ldr lr, [r0, #4]!
        /* can use ldmia from now on..*/
        add r0, r0, 4
        /* load spsr */
        ldmia r0!, {r1}
        msr spsr, r1
        /* restore registers, pc and cpsr (from spsr) */
        ldmia r0, {r0-r12, pc}^
        /* 
        sprinkle nops just in case..
         */
        nop
        nop
        nop
        nop
        nop
        nop
        nop
        "
        :: "r"(&c)
        :: "volatile")
    
    };    
}

#[naked]
extern "C" fn vector_entry() -> !{
    // lr - 4 points to where we need to return
    // original lr and sp are saved on diff mode
    // save all the registers (http://simplemachines.it/doc/arm_inst.pdf)
    // STMFD sp!,{r0-r12, lr}
    // save also spsr, just incase of context switch
    // implement sp, lr = get_orig_sp_lr();
    // then we can save the context!
    // save spsr incase we are gonna need it.
    // r13,r14 are saved in the cpu when we switch modes else; r15 will be the current r14

    unsafe{
    asm!("sub lr,lr, #4
          push {lr}
          push {r0-r12}
          mrs r1, spsr
          push {r1}

          /* prepare argument for next function */
          mov r0, sp
          /* restore stack to the original location */
          /* this is a bit weird as i am going to use the 'freed' stack soon..
          but interrupts are disable so should be ok
          */
          add sp, sp, $1
          /* switch to the mode we came from, if it user mode, change to system mode */

          /* get the rest of the control bits */
          mrs r2, cpsr
          /* mask everything but control */
          and r2, r2, #0xFF
          /* remove the mode bits */
          bic r2, r2, $2

          /* mask the mode */
          and r1, r1 , $2
          /* check if user mode */
          cmp r1, $4
          /* if user mode, change to system mode */
          moveq r1, $5
          /* add the other flags */
          orr   r1, r1, r2
          /* change back to original mode to grab sp and lr */
          msr cpsr_c, r1

          /* save original lr and sp */
          str lr, [r0, #-4]!
          str sp, [r0, #-4]!


          /* set to supervisor mode */
          mov r1, $3
          /* add the other flags */
          orr   r1, r1, r2
          /* change mode! */
          msr cpsr_c, r1
          /* move on */
          bl $0
          /* should not get here */
    ":: "i"(vector_with_context as extern "C" fn(_) ),
        "i"(SIZE_OF_INT_CTX - 2*4 /* lr and sp are pushed after stack is fixed */),
        "i"(super::cpu::MODE_MASK),
        "i"(super::cpu::SUPER_MODE),
        "i"(super::cpu::USER_MODE),
        "i"(super::cpu::SYS_MODE)
        :: "volatile");
    }
    loop{}
}

vector_entry
    }}
}

/*
 this function is never called, i just it to avoid more assembly files
*/
#[naked]
fn vector_table_asm() {
    unsafe {
        asm!("ldr pc, [pc, #24]" : : : : "volatile");
    };
}

// TODO change vec_table to point to the right place in 
// TODO: make vectable not static and part of platform services.
pub fn init_interrupts() {
    unsafe {
        let mut vec_table: &'static mut [u32] =
            slice::from_raw_parts_mut(VECTORS_ADDR.0 as *mut u32, 4 * 8 * 2);

        let asmjump: *const u32 = vector_table_asm as *const u32;
        vec_table[0] = *asmjump;
        vec_table[1] = *asmjump;
        vec_table[2] = *asmjump;
        vec_table[3] = *asmjump;
        vec_table[4] = *asmjump;
        vec_table[5] = 0;
        vec_table[6] = *asmjump;
        vec_table[7] = *asmjump;

        // default implementations
        vec_table[8 + 0] = inthandler!(vector_reset_handler) as u32;
        vec_table[8 + 1] = inthandler!(vector_undefined_handler) as u32;
        vec_table[8 + 2] = inthandler!(vector_softint_handler) as u32;
        vec_table[8 + 3] = inthandler!(vector_prefetch_abort_handler) as u32;
        vec_table[8 + 4] = inthandler!(vector_data_abort_handler) as u32;
        vec_table[8 + 5] = 0;
        vec_table[8 + 6] = inthandler!(vector_irq_handler) as u32;
        vec_table[8 + 7] = inthandler!(vector_fiq_handler) as u32;
    }
}

pub struct VectorTable<InterruptableT: Borrow<platform::InterruptableWithContext>> {
    // TODO re-write when we have a heap
    irq_callback: Option<InterruptableT>,
}

// TODO: sync access to this or even better, to it lock free :)
static mut VEC_TABLE: VectorTable<Box<platform::InterruptableWithContext>> = VectorTable { irq_callback: None };

// TODO: make thread safe if multi processing !!
pub fn get_vec_table() -> &'static mut VectorTable<Box<platform::InterruptableWithContext>> {
    unsafe { &mut VEC_TABLE }
}

impl<InterruptableT: Borrow<platform::InterruptableWithContext>> VectorTable<InterruptableT> {
    pub fn set_irq_callback(&mut self, callback: InterruptableT) {
        self.irq_callback = Some(callback);
    }
}

fn vector_reset_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    platform::write_to_console("reset interrupt!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);

    // TODO : call scheduler
    loop {}

}

fn vector_undefined_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    // data about is lr - 8; the macro gave us lr -4 in pc, so just fix the missing 4 bytes

    ctx.pc -= 4;

    platform::write_to_console("Undefined abort!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);
    loop {}
}

fn vector_softint_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    platform::write_to_console("softint handler!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);
    loop {}
}

fn vector_prefetch_abort_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    platform::write_to_console("prefetch abort!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);

    loop {}
}

fn vector_data_abort_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    // data about is lr - 8; the macro gave us lr -4 in pc, so just fix the missing 4 bytes

    ctx.pc -= 4;

    platform::write_to_console("Data abort!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);

    loop {}
}

fn vector_irq_handler(ctx: &mut InterruptContext) {
    unsafe {
        if let Some(ref mut func) = VEC_TABLE.irq_callback {
            func.interrupted_ctx(ctx);
        }
        platform::get_platform_services().post_interrupted(ctx);
    }
}

fn vector_fiq_handler(ctx: &mut InterruptContext) {
    use collections::String;
    use core::fmt::Write;
    platform::write_to_console("fiq interrupt!");
    let mut w = String::new();
    write!(&mut w, "Context: {:?}", ctx);
    platform::write_to_console(&w);
    loop {}
}
