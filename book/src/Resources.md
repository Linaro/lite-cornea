# Resources

Iris Resources are a collection of hardware registers and
invocation parameters specific to that hardware.

Cornea allows for resource discovery with the resource-list
subcommand. For example, the following prints a table
describing the resources of the host-cluster0, cpu0:
```bash
$ cornea resource-list host.cluster0.core0
type  │ bits │                 name │ description
══════╪══════╪══════════════════════╪═════════════════════
Reg   │   32 │          PC_MEMSPACE │ Iris memory space id of the current PC and the current SP.
Reg   │   64 │                   X0 │ X0
Reg   │   64 │                   X1 │ X1
Reg   │   64 │                   X2 │ X2
---- etc. ----
```

These resources may be accessed by name with the resource-
read subcommand. For example, the following prints the PC of
the host cpu0:

```bash
$ cornea resurce-read host.cluster0.cpu0 PC
   value │ name
═════════╪════════════════════════════════════
       0 │ PC_MEMSPACE
       0 │ PC
```

Furthermore, to look at the semihosting parameters for the scp cpu:

```
$ cornea resource-read css.scp.cpu semihosting
   value │ name
═════════╪════════════════════════════════════
       1 │ semihosting-enable
      ab │ semihosting-Thumb_SVC
       0 │ semihosting-heap_base
20700000 │ semihosting-heap_limit
20800000 │ semihosting-stack_base
20700000 │ semihosting-stack_limit
       0 │ semihosting-prefix
```
