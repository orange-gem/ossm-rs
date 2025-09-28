## OSSM-RS

And alternative firmware for OSSM written in rust.

You can find the original hardware and software [here](https://github.com/KinkyMakers/OSSM-hardware/tree/master)

### Features

- Full RS485 based motor control
- Motor settings set automatically
- S-curve motion planning
- Strict mechanical bounds checks
- Adjusting depth, velocity, and stroke start on the fly

### Remote Support

- [M5 remote](https://github.com/ortlof/OSSM-M5-Remote)

### Trying it out

Install the toolchain for xtensa devices using the instructions from [here](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html#xtensa-devices)

Compile with and upload with:
```
cargo run --release
```

### Roadmap

- [ ] Patterns
- [ ] R&D wireless remote support
- [ ] Off-the-shelf control board support
