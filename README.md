# Bevy Multiplayer Example
Run the server with: `cargo run -- server`  
Run the client with: `cargo run -- client`

## TODO:
- Fix bug where client prediciton runs only when receiving a player sync
on that frame.
- Clean up and document grounded, jumping logic, etc.
- Move all player specific data to a single struct, not distributed around.
- Applying inputs and kinematics are kind of similar, they should be the same 
function. Or create a function that applies kinematics for an entity, and call it
after applying input. applying input would then move the player, and set kinematics.
These kinematics would be run only in the kinematic system, or right after the 
input (except for the last one since the normal kinematic system would catch it)
