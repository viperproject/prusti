/// An adaptation of this example:
/// https://play.rust-lang.org/?version=nightly&mode=debug&edition=2018&gist=b8dfafd14113f2933c1b5127c861df44
///
/// which was created by Matsakis to show-case NLL in his blog:
/// http://smallcultfollowing.com/babysteps/blog/2018/10/31/mir-based-borrowck-is-almost-here/
///
/// This example illustrates the differences between the lexical borrow
/// checker from Rust 2015 edition and the new non-lexical borrow
/// checker that was shipped as part of the Rust 2018 edition.
///
/// Changes:
///
/// +   Changed built-in types with mock types.
/// +   Rewrote loops into supported shape (while bool with no break, continue, or return).
///
/// Verified properties:
///
/// +   Absence of panics.

/* COUNTEREXAMPLE: 
    fn router(me, rx, tx):
        me <- StringWrapper{
            value: "hello"
        },
        rx <- Receiver<Message>{
            receiver: ? (some instance)
        },
        tx <- Sender<Message>{
            sender: ?
        },
        message_option <- MessageOption::Some(Message::Letter {
            recipient: StringWrapper {
                value: "world"
            }, 
            data: StringWrapper {
                value: "something"
            }
        }),
        is_some <- true,
        message <- Message::Letter {
            recipient: StringWrapper {
                value: "world"
            }, 
            data: StringWrapper {
                value: "something"
            }
        },
        t0('result of tx.send(message)') <- SResult::Err
        
    (this last variable is something new but useful, an intermediate
    result after calling tx.send(message). Without this the problem 
    of this program is hard to determine from the counterexample)


    (does the reassignment do anything?)
*/




use prusti_contracts::*;

use std::sync::mpsc;

struct StringWrapper {
    value: String,
}

impl StringWrapper {

    #[pure]
    #[trusted]
    fn equals(&self, other: &StringWrapper) -> bool {
        self.value == other.value
    }
}

enum Message {
    Letter { recipient: StringWrapper, data: StringWrapper },
}

struct Receiver<T> {
    receiver: mpsc::Receiver<T>,
}

impl Receiver<Message> {

    #[trusted]
    pub fn recv(&mut self) -> MessageOption {
        match self.receiver.recv().ok() {
            Some(msg) => MessageOption::Some(msg),
            None => MessageOption::None,
        }
    }

}

struct Sender<T> {
    sender: mpsc::Sender<T>,
}

enum SResult<T> {
    Ok(T),
    Err,
}

impl<T> SResult<T> {
    #[pure]
    fn is_ok(&self) -> bool {
        match self {
            SResult::Ok(_) => true,
            SResult::Err => false,
        }
    }
    #[requires(self.is_ok())]
    fn unwrap(self) -> T {
        match self {
            SResult::Ok(v) => v,
            SResult::Err => unreachable!(),
        }
    }
}

impl Sender<Message> {
    #[trusted]
    pub fn send(&mut self, msg: Message) -> SResult<()> {
        match self.sender.send(msg) {
            Result::Ok(_) => SResult::Ok(()),
            Result::Err(_) => SResult::Err,
        }
    }
}

enum MessageOption {
    Some(Message),
    None,
}

impl MessageOption {

    #[pure]
    pub fn is_some(&self) -> bool {
        match self {
            MessageOption::Some(_) => true,
            MessageOption::None => false,
        }
    }

    #[requires(self.is_some())]
    pub fn take(self) -> Message {
        match self {
            MessageOption::Some(msg) => msg,
            MessageOption::None => {
                unreachable!();
            }
        }
    }

}

fn router(me: &StringWrapper, rx: Receiver<Message>, tx: Sender<Message>) {
    let mut rx = rx;
    let mut tx = tx;
    let mut message_option = rx.recv();
    let mut is_some = message_option.is_some();
    while is_some {
        body_invariant!(message_option.is_some());
        let mut message = message_option.take();
        match &message {
            Message::Letter { recipient, data } => {
                if recipient.equals(me) {
                    // The program does not handle the case in which `send()` fails, so the
                    // `unwrap()` might panic.
                    tx.send(message).unwrap(); //~ ERROR precondition might not hold.
                } else {
                    process(data);
                }
            }
        }
        message_option = rx.recv();
        is_some = message_option.is_some();
    }
}

fn process(_data: &StringWrapper) {}

fn main() { }
