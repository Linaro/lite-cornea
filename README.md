# lite-cornea
A command line shim to interact with the Iris Debug server of an ARM FVP

/!\ This is alpha /!\

For the time being, run the help command `cargo run help` or `cornea help` to figure out usage.

Be aware that you need to add the `-I` option to the Fast Model/FVP command line.
As far as I can tell, there's not default way to add this parameter to a west build from the command line.
I have added it manually to `ARM_FVP_FLAGS` in `board.cmake` within my board's directroy.
This is not an ideal solution, so it would be nice to standardize something.

## A note on instances

FVPs and Fast Models are built on 'instances', which simulate an individual ip block
or ip block connection.
Instances are named within a model are organized in a tree-like manner and
named separating layers with a `.`.
Many subcommands of cornea accept an instance as a parameter.

## Usage Examples

The following examples all use the FVP for the Corstone1000.

Printing the children instances of the secure element in the heirachy:

```bash
$ cornea child-list component.IoT_Corstone_1000.se
.cpu
.ClkCtrl.clkSelect
.ClkCtrl.clkGate
.ClkCtrl
.cpu_labeller
.SysCtrlRegs
.ChassisCtrlRegs
.uart0
---- etc. ----
```

Print a table describing the resources of the host-cluster0, cpu0:
```bash
$ cornea resource-list component.IoT_Corstone_1000.host.cluster0.core0
type  │ bits │                 name │ description
══════╪══════╪══════════════════════╪═════════════════════
Reg   │   32 │          PC_MEMSPACE │ Iris memory space id of the current PC and the current SP.
Reg   │   64 │                   X0 │ X0
Reg   │   64 │                   X1 │ X1
Reg   │   64 │                   X2 │ X2
---- etc. ----
```

Printing the PC of the host cpu0:

```bash
$ cornea resurce-read component.IoT_Corstone_1000.host.cluster0.cpu0 PC
   value │ name
═════════╪════════════════════════════════════
       0 │ PC_MEMSPACE
       0 │ PC
```

Reading 100 bytes of memory at address 0 as seen by the flash memory
grouped into u16 sized entries:

```
$ cornea memory-read component.IoT_Corstone_1000.board.flash0 6 8 --group-by u16
         0    2    4    6    8    a    c    e
00000000                e800 0010 e7ff e800            ........
```

Create a gdb connection to the model from the persepctive of the secure
enclave processor:

```
#!/bin/env -S gdb -ix
set arch armv6-m
target remote | cornea gdb-proxy component.IoT_Corstone_1000.se.cpu
```
Note: The above is a script that, when made executable, will connect to the
model automatically, similar to the behavior when you run gdb with an executable
parameter or a process id flag.
