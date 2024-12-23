# Bevy Multiplayer Example
Run the server with: `cargo run -- server`  
Run the client with: `cargo run -- client`
Run the demo of the procedural generation with `cargo run -- proc`


## Structure
- **`src`**: Root directory of the project.
  - **`client`**: Starts the client, handling connections and updates from the server.
  - **`server`**: Starts the server, managing game state and client connections.
  - **`main`**: Chooses to run the client or server based on a command-line flag.
  - **`messages/`**: Defines client and server messages and their parsers.
  - **`shared/`**: Contains shared game logic.
    - **`proc/`**: Contains logic for procedural terrain generation:
    - **`objects/`**: Each object handles its own spawning and syncing logic:
      - **Server**: Spawning and syncing objects.
      - **Client**: Receiving spawns and syncs.

### Player
The player is a special case as they have predicted input. The player reads input
from the user, immediately reacts to the input, then sends the input to the server.
The server then stores the input and applies it, then broadcasts a response with
the information about the player after the input (e.g. the new position). The client
then receives this and updates the players position (if there is a discrepency).
If there is, then the client will rollback to the state, then replay any new inputs
to get back to the current state. See `recv_position_sync` in `src/shared/objects/players/client.rs`
for the implementation.


## TODO:
- Change message readers to use bevy's event system
- Muzzle flash, bullet decals, tracers
- Render gun with different camera
- Gun animations (recoil, sprint wobble)
- Damage flinch
