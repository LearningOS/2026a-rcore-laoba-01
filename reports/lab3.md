# Lab3 实验报告

> 姓名：徐华宇　学号：3248921201　GitHub：laoba-01

## 一、功能总结

在 ch5 内核（已有 fork/exec/waitpid/进程树/独立地址空间）的基础上，完成了 lab3 要求的四项工作：迁移 `sys_get_time` / `sys_mmap` / `sys_munmap`，新增 `sys_spawn`（ID 400）与 `sys_set_priority`（ID 140），并把 FIFO 调度器替换为 stride 调度。

- **`sys_get_time`**：用当前进程页表把 `TimeVal *` 翻译成物理地址。`sec` 与 `usec` 分别调用 `translated_refmut`，因此 `usec`（`va + 8`）落在相邻物理页时也能正确写入，不依赖"两个字段在同一物理页"的假设。
- **`sys_mmap`**：校验 `start` 页对齐、`port ∈ [1,7]`；用 `checked_add` 计算结束地址防止 `usize` 溢出回绕；按 port 的 bit0/1/2 组装 `MapPermission` 并补上 U 位；插入前用 `is_overlapped` 拒绝与已有 `MapArea` 重叠的请求，最后 `insert_framed_area`。
- **`sys_munmap`**：要求 `start` 页对齐且 `len != 0`，同样用 `checked_add` 计算结束地址；在 `MemorySet` 中按"起始 VPN 与结束 VPN 都完全吻合"查找并移除 `MapArea`，找不到返回 `-1`。移除时 `FrameTracker` 随之析构，物理帧自动归还分帧器。
- **`sys_spawn`**：先从用户地址空间读出路径字符串，查 ELF 数据；用 `TaskControlBlock::new` 新建一套地址空间（不复制父进程内存），再手动把新进程挂到当前进程的 `children` 下、把 `parent` 设为当前进程的 `Weak` 引用，最后加入就绪队列并返回 pid。
- **`sys_set_priority`**：`prio < 2` 时返回 `-1`；否则更新 `priority`，按 `pass = BIG_STRIDE / prio`（下限 1）重算步长，返回新的优先级。
- **stride 调度**：`TaskControlBlockInner` 增加 `priority` / `stride` / `pass` 三个字段；新进程默认 `priority = 16`、`stride = 0`、`pass = BIG_STRIDE / 16`；`fork` 时子进程继承父进程的三个值；`TaskManager::fetch` 改为遍历就绪队列选出 `stride` 最小的进程，取出后执行 `stride += pass`，队列为空时返回 `None`。

验证结果：

- 本地 `ch5_usertest` 全部通过，输出 `ch5 Usertests passed!`；`ch4_mmap1` / `ch4_mmap2` 按预期触发 PageFault 被内核以退出码 `-2` 杀死。
- stride 测例中六个进程的 `count / priority` 分别为 2.70M、2.79M、2.80M、2.82M、2.82M、2.78M，基本正比于各自优先级。
- 官方 checker（`make -C ci-user test CHAPTER=5`）输出 `Test passed: 15/15`。

## 二、关键设计与取舍

### 1. spawn 为什么不等于 fork + exec

`fork` 会复制父进程的整个地址空间，`exec` 会把**当前进程**的地址空间替换掉。若用二者拼出 spawn，调用者自己的地址空间会被覆盖，调用后也无法继续执行。spawn 的语义是"新建一个跑指定程序的进程，调用者不受影响"，因此直接 `TaskControlBlock::new(elf_data)` 建立全新地址空间，再由内核把它挂进进程树。

模板里的 `TaskControlBlock::new` 是为 initproc 服务的，`parent` 字段写死为 `None`；spawn 必须补齐父子双向关系，否则 `waitpid` 在自己的 `children` 里找不到这个进程。

### 2. stride 调度的三个字段

`priority` 是权重，`pass = BIG_STRIDE / priority` 是每次被调度时 `stride` 的增量，调度器每次选 `stride` 最小的进程。优先级越高，`pass` 越小，`stride` 增长越慢，被选中的次数就越多，长期占比趋近于优先级之比。`BIG_STRIDE` 取 `100_000` 这类较大的常数，是为了让 `BIG_STRIDE / prio` 的整数截断误差占比更小。

两个边界：`prio` 很大时 `BIG_STRIDE / prio` 会截断成 0，该进程的 `stride` 永不增长，会独占 CPU，因此用 `.max(1)` 兜底；`stride` 是 `usize`，长时间运行可能溢出，`BIG_STRIDE` 不宜取得过大。

### 3. 与 lab2 的差异

lab2 的 `munmap` 支持砍头/砍尾/中间挖洞四种情况；本章测例只要求整段完全匹配（`ch4_unmap2` 明确要求长度或起始不匹配时返回 `-1`），故实现为精确匹配整段。`mmap` 的重叠检查在 lab2 已经做过，本章补在模板缺失的位置上，否则 `ch4_mmap3` 会在页表 `map()` 的断言处直接 panic。

## 三、问答作业

### Q1 为什么 spawn 不能直接用 fork + exec 实现？

因为两者操作的对象不同。`fork` 复制的是**调用者**的地址空间，`exec` 替换的也是**调用者**的地址空间；而 spawn 要创建的是一个**新的**进程。若用 fork + exec 实现，exec 会覆盖调用者自己，调用者无法在 spawn 之后继续执行，也拿不到"被创建进程"与"自己"的区分。此外 fork 先复制父进程内存再全部丢弃，在 spawn 场景下是无谓开销。

本内核的 spawn 直接 `TaskControlBlock::new(elf_data)`，从 ELF 构建全新地址空间，并把新进程挂到当前进程的 `children` 中，二者互不影响。

### Q2 简述 stride 调度算法的原理

每个进程维护一个虚拟时间 `stride`，每次被调度时加上自己的步长 `pass`；调度器每次挑选 `stride` 最小的进程运行。`pass` 与优先级成反比（`BIG_STRIDE / priority`），所以高优先级进程的 `stride` 增长慢，会更频繁地被选中，最终各进程获得的 CPU 时间与其优先级成正比。该算法实现简单，且不需要像时间片轮转那样维护队列。

### Q3 set_priority 的实现要点

`set_priority(prio)` 只接受 `prio >= 2`，因为优先级上限（步长）不能为 0；Linux 的 nice 值也把最低权重限制为正数，避免某个进程永远得不到调度。实现上更新 `priority` 后必须同步重算 `pass`，否则新优先级不会生效——调度器只看 `pass` 和 `stride`，不看 `priority` 本身。返回值按测例约定是**新的**优先级，而不是旧值。

### Q4 定时器中断如何参与 stride 调度？

内核在 S 态开启了 `stimer` 中断（`sie::set_stimer`），每个 trap tick 由 `set_next_trigger` 设定下一次定时中断。定时中断到来时 `trap_handler` 调用 `suspend_current_and_run_next`，当前进程回到就绪队列，下一次 `fetch` 就会重新选择 `stride` 最小的进程。因此 stride 调度天然是抢占式的，无需进程主动让出 CPU。

### Q5 进程退出后，其子进程如何处理？

`exit_current_and_run_next` 会把退出进程的所有子进程挂到 `INITPROC` 的 `children` 下，并更新它们的 `parent` 指向 initproc（`task/mod.rs` 中实现）。这样孤儿进程不会因为父进程的 TCB 被回收而失去回收者，最终由 initproc 通过 `waitpid` 收尸。

## 四、荣誉准则

我承诺遵守学术诚信准则：本报告内容真实，实验代码与结果可复现；引用教材、课程资料及外部帮助之处均已注明，未抄袭他人代码或报告，未伪造测试结果。

## 五、可选反馈

本章把进程、地址空间和调度器串了起来，stride 调度本身只有十几行，但"优先级只在 `pass` 上体现"这一点如果没有想清楚，很容易出现"设置了优先级却毫无变化"的困惑。建议测例说明里点明 `set_priority` 的返回值语义（是设置后的新值），以及 `mmap` 对重叠区间必须返回错误而不是覆盖。
