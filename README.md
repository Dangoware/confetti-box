# Confetti-Box 🎉
A super simple file host. Inspired by [Catbox](https://catbox.moe) and [Uguu](https://uguu.se).

## Features
### Current
- Entirely self contained, tiny (~4MB) single binary 
- Customizable using a simple config file
- Only stores one copy of a given hash on the backend
- Fast (enough), runs just fine on a Raspberry Pi
- Simple API for interfacing with it programmatically
- No database setup required, uses self-contained in memory database
  serialized to a small, LZ4 compressed file.

### Planned
- Chunked uploads
- Theming
- More mochi

## Screenshot
<p align="center">
  <img width="500px" src="https://github.com/user-attachments/assets/2ca05da6-ce5e-4cde-a3b0-453ccdd33264">
  <p align="center"><i>An example of a running instance</i></p>
</p>

## License
Confetti-Box is licensed under the terms of the GNU AGPL-3.0 license. Do what you want
with it within the terms of that license.
