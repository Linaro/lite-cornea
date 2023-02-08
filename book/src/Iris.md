# A quick IRIS primer

IRIS organizes hardware into discrete units called instances.
Many operations, such as memory and register reads and writes,
are performed on an instance and happen from that instance's
perspective, as if the hardware the instance is emulating were
to do the request itself.

Instances are conventionally organized into a heirachy.
The names of instances reflect it's position in this hierachy,
starting with the root, and moving down the tree, separating
each layer with a '.'.

# Inspecting the heirachy

On the command line, cornea can inspect the instance hierachy
with the `child-list` subcommand.

For example, listing the children instances of the secure element
within the Corstone1000 model looks like:

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

When no parent component is specified, all instances are printed.
This subcommand can be useful when piped to a search tool such
as grep to find instances that are important like a cpu:

```
$ cornea child-list | rg 'cpu0?$'
component.TC2.css.scp.cpu
component.TC2.css.rss.cpu
component.TC2.css.cluster0.subcluster0.cpu0
component.TC2.css.cluster0.subcluster1.cpu0
```

