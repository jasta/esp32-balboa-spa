//! Protocol handling for Balboa Spa family of products, including a mock main board implementation
//! for software testing automation and validation.

pub mod main_board;
pub mod mock_spa;
mod channel_tracker;
mod timer_tracker;
mod clear_to_send_tracker;
pub mod channel_manager;
