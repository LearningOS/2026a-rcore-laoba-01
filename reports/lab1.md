# Lab1 实验报告

> 姓名：徐华宇　学号：3248921201　GitHub：laoba-01

## 一、功能总结

在 ch3 内核中实现了 `sys_trace` 系统调用（ID 410）。在 `TaskControlBlock` 中为每个任务维护 `syscall_times` 数组，在 `syscall()` 入口统一计数；`sys_trace` 支持读/写用户内存和查询系统调用次数。读写用 volatile 指针直接访问用户地址；ch3 无地址空间，故不做检查。`BASE=2` 下 QEMU 输出出现 `Test trace OK!`，13 个程序全部跑完。

## 二、简答作业

### Q1 三个 bad 测例

1. `ch2b_bad_address`：用户程序向地址 `0x0` 写入。ch3 没有地址空间隔离，`0x0` 不可写，触发 Store/Page Fault。内核输出 `PageFault in application, bad addr = 0x0, bad instruction = 0x804003a4, kernel killed it.`，随后终止该任务；程序里的 `panic!("FAIL")` 不会执行。

2. `ch2b_bad_instructions`：用户程序在 U 态执行 `sret`。`sret` 是 S 态特权指令，在 U 态执行会触发非法指令异常，内核输出 `IllegalInstruction in application, kernel killed it.` 并终止任务。

3. `ch2b_bad_register`：用户程序在 U 态执行 `csrr ..., sstatus` 读取 S 态 CSR。U 态无权访问 S 态寄存器，同样触发非法指令异常，内核终止任务。

上述现象说明：进入 U 态后，程序不能执行 S 态特权指令，也不能访问 S 态 CSR；一旦违反，硬件会产生 trap，由内核捕获并处理。本次实验使用的 SBI 为 **RustSBI 0.4.1**（启动日志中可见）。

### Q2 `trap.S` 中的 `__alltraps` 与 `__restore`

1. 刚进入 `__restore` 时，`sp` 指向内核栈上保存的那份 `TrapContext` 的开头，里面是 32 个通用寄存器、`sstatus` 和 `sepc`。`__restore` 有两种使用情景：

   - 从 trap 返回：`__alltraps` 保存上下文并调用 `trap_handler`，返回后继续执行 `__restore`，此时 `sp` 仍指向刚保存的 `TrapContext`；
   - 任务首次运行或任务切换：`TaskContext::goto_restore` 把 `ra` 设为 `__restore`，`sp` 设为 `KernelStack::push_context` 返回的 `TrapContext` 地址；`__switch` 切换过去后从 `__restore` 开始，恢复该任务的上下文。

2. L43–L48 特殊处理三个 CSR：

   - `sstatus`：恢复 SPP 和中断相关状态。SPP 决定 `sret` 返回到哪个特权级，这里为 User；
   - `sepc`：用户态要返回的 PC，`sret` 会跳转到它；
   - `sscratch`：先放用户栈指针，等 L60 那次 `csrrw` 交换时用；换完之后它又变回内核栈顶，留给下一次 trap。

3. L50–L56 跳过 `x2` 和 `x4`：

   - `x2` 是用户栈指针。它已经在 L48 通过 `sscratch` 准备好，最后由 L60 交换回 `sp`；如果在这儿直接恢复 `x2`，内核栈指针就被覆盖了，后面的 `ld` 就找不到 `TrapContext` 了；
   - `x4` 是线程指针 `tp`，用户程序不使用 TLS，因此不需要保存和恢复；
   - `x0` 恒为 0，也不需要处理。

4. L60 `csrrw sp, sscratch, sp` 之前，`sp` 是内核栈顶（L58 已释放 `TrapContext`），`sscratch` 是用户栈指针。交换之后，`sp` 变成用户栈指针，`sscratch` 变成内核栈顶。随后 `sret` 在用户栈上返回用户态，下一次 trap 进入时再由 L13 交换回来。

5. 状态切换就在 `sret` 这一条。它把 PC 改成 `sepc`，特权级按 `sstatus.SPP` 来；这里 SPP 是 User，所以执行完就回到 U 态，同时按 SPIE 恢复中断。

6. L13 `csrrw sp, sscratch, sp` 之前，`sp` 是用户栈指针，`sscratch` 是内核栈顶。交换之后，`sp` 变成内核栈顶，`sscratch` 变成用户栈指针；这样内核可以在自己的栈上保存 `TrapContext`，而用户栈指针稍后被保存到 `x2` 槽。

7. 从 U 态进入 S 态是由 trap 引起的，不是 `__alltraps` 中的某条指令。系统调用时，用户程序执行 `ecall`，硬件产生环境调用异常，切换到 S 态并跳转到 `stvec` 指向的 `__alltraps`；异常和中断也由硬件在相应位置自动完成特权级切换。进入 S 态后执行的第一条指令是 L13 的 `csrrw sp, sscratch, sp`。

## 三、荣誉准则

1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 **以下各位** 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

    与 Claude（AI 编程助手）就本实验的代码实现、`trap.S` 机制和报告撰写进行过交流。

2. 此外，我也参考了 **以下资料** ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

    无。

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。
我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。
我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。
我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。
我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。
