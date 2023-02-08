# Resources

Iris Resources are very similar to registers, and most
hardware registers are available as resources on instances.

Cornea allows for resource discovery with the resource-list
subcommand. For example, the following prints a table
describing the resources of the host-cluster0, cpu0:
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

These resouurces may be accessed by name with the resource-
read subcommand. For example, the following printsthe PC of
the host cpu0:

```bash
$ cornea resurce-read component.IoT_Corstone_1000.host.cluster0.cpu0 PC
   value │ name
═════════╪════════════════════════════════════
       0 │ PC_MEMSPACE
       0 │ PC
```
