# lite-cornea
A command line shim to interact with the Iris Debug server of an ARM FVP

/!\ This is alpha /!\

For the time being, run the help command `cargo run help` or `cornea help` to figure out usage.

Be aware that you need to add the `-I` option to the Fast Model/FVP command line.
As far as I can tell, there's not default way to add this parameter to a west build from the command line.
I have added it manually to `ARM_FVP_FLAGS` in `board.cmake` within my board's directroy.
This is not an ideal solution, so it would be nice to standardize something.
