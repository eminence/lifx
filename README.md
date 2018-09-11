LIFX
====

LIFX bulbs are wifi-controlled RGBW light bulbs.  The bulbs can be controlled directly
by sending packets to them over the LAN, or they can be controlled over the internet
via an HTTP API.

This library provides functionality for inspecting and controlling the bulbs over the
LAN only.

The LIFX protocol docs can be found [here](https://lan.developer.lifx.com/).

lifx-core
---------

This library implements all the data structures and utilities for inspecting and
constructing the low-level control packets.  It does not deal with the actual sending
or receiving of bytes from the network.

Supported LIFX products:

- [x] Light bulbs
- [x] Multizone devices (LIFX Z and Beam)
- [ ] Tile devices





Higher level library
--------------------

Eventually this library will also include a higher-level library that will take care
of talking with the network, maintaining bulb state, etc.  But this isn't ready yet.



License and terms
=================

This library code is licensed under either of:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

Much of the documentation in this library is taken from the LIFX protocol docs.
Using this library to communicate with LIFX bulbs likely binds you to the
[LIFX Developer Terms](https://www.lifx.com/pages/developer-terms-of-use).