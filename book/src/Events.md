# Events

Cornea may listen for certain events provided by instances.
The following subcommands interact with events:
 * `event-sources` - List and describe events that an instance
   may emit.
 * `event-fields` - Describe the structure of an event
 * `event-log` - Print events in JSON as they happen

In all the examples in this chapter, our goal will be to read
the uart traffic, line-by-line, from uart0.

# Sources

Event sources vary between instances, and many instances
do not produce events.
Cornea is able to list event sources per-instance with the
`event-sources` subcommand.

For example, we can look through the events provided by uart0:
```
$ cornea event-sources bp.pl011_uart0
                    name │ description
═════════════════════════╪═════════════════════
      CHECKPOINT_MESSAGE │ Report error messages from the checkpointing process
  CHECKPOINT_RESTORE_END │ Checkpoint restore completed
CHECKPOINT_RESTORE_START │ Checkpoint restore about to start
     CHECKPOINT_SAVE_END │ Checkpoint save completed
   CHECKPOINT_SAVE_START │ Checkpoint save about to start
      pl011_character_in │ A character was received.
     pl011_character_out │ A character was written to the UART to output.
 pl011_line_buffered_out │ The buffered output of character_out.  The buffer is flushed when any control character is received.  The control characters do not form part of the buffer.  The buffer's size is limited to 255 characters and truncation is indicated by an appended '...'
 ```

 With this, we can tell from the name and description, that we
 are interested in the event `pl011_line_buffered_out`.

 # Fields

 Events are a structured data format, that may differ for each event on each instance.
 Understanding the structure of an event is critical for understanding
 an event log.
 Cornea queries the event structure with the `event-fields` subcommand.

 Continuing with out uart0 example:
 ```
$ cornea event-fields bp.pl011_uart0 pl011_line_buffered_out
type  │ size │                 name │ description
══════╪══════╪══════════════════════╪═════════════════════
uint  │    8 │                 tick │ The count of ticks from simulation start that the UART has received at the point at which it receives the control character that flushes the buffer.
string│    0 │               buffer │ The line buffer.
```

This event is straightforward: it contains a timestamp and a line as
written to the uart.

# Log

The final step in events is logging them.
Cornea provides the `event-log` subcommand for this purpose.

Continuing our uart example:
```
$ cornea event-log bp.pl011_uart0 pl011_line_buffered_out
{"esId":0,"fields":{"buffer":"NOTICE:  Booting Trusted Firmware","tick":10},"instId":649,"sInstId":104,"time":982880000}
{"esId":0,"fields":{"buffer":"NOTICE:  BL1: v2.9(debug):v2.9.0-353-g2503c8f32-dirty","tick":11},"instId":649,"sInstId":104,"time":1016500000}
```

This may not be the most readable output, so we can use jq to clean it up:
```
$ cornea event-log bp.pl011_uart0 pl011_line_buffered_out \
| jq .fields.buffer
"INFO:    Loading image id=13 at address 0x4003000"
"INFO:    Image id=13 loaded: 0x4003000 - 0x4003244"
"INFO:    Loading image id=3 at address 0x4003000"
"INFO:    Image id=3 loaded: 0x4003000 - 0x40151dd"
"INFO:    BL2: Loading image id 23"
"INFO:    Loading image id=6 at address 0x7f00000"
"INFO:    Image id=6 loaded: 0x7f00000 - 0x7f002cb"
```

Much nicer. Quite an unusual route to the uart output though.

