# BlazeFTC

BlazeFTC is a (partial) Rust rewrite of the binary protocol used by the REV Robotics Control Hub and Expansion Hub used in *FIRST®* Tech Challenge, a high school robotics competition. 
Designed for speed, it offers the fastest possible loop times, thanks to it's direct hardware control, faster programming language, and "reactive" design. 
It is capable of running in the context of a regular opmode, and it maintains compatibility with the FTC SDK, which can still issue commands while the BlazeFTC opmode is running.

**As of now, it should be considered to be in pre-alpha.** Most of the code is there, but there's some bugs and design choices I would like input on, among other things. Actionable suggestions/pull requests are appreciated.

The quickstart can be found [here](https://github.com/owenpmckenna/BlazeFtcQuickstart), but please read the explanations below before you start.

### What's going on?

When the BlazeFTC opmode is started, we use Java's reflection APIs to obtain references to the device file (`/dev/ttyS1`) that the Control Hub's compute board uses to communicate with the Lynx Module (the board with the motor drivers, servo drivers, sensor ports, etc).
We replace it with a proxy Input/OutputStream, backed by JNI calls to our Rust code which allows the FTC SDK to continue communicating. We deserialize and reserialize all the frames, which is how the protocol was reverse engineered and tested in the first place.

In the normal SDK, IO is fundamentally blocking. Directly controlling the hardware allows us to throw that out the window, and send commands at speeds apparently limited only by the UART baud rate.
In testing, an opmode with 4 motor PID loops was running at ~500hz, just to give an idea of the speed we can accomplish here.

### Ok, but why?

Speed. Most normal opmodes run at ~30hz, even with bulk reads enabled. Charitably to the SDK, a moderately optimized BlazeFTC opmode should be able to run at about ~150hz minimum, if not much faster.
This is significant for all manner of control algorithms, which at a broad level all need to react to error in the real world. Whether this error is from gravity/friction, motor inaccuracy, someone being in your way, etc., BlazeFTC can react 5 or more times faster to real world conditions.
Also, of course, we get to code in Rust now. Whether you think that's a good thing is up to you (personally I think it's awesome but I'm the kind of person who will spend their entire winter break writing something like this).

### Robot Framework

While you *can* write bare opmodes, I highly suggest that you use the Robot framework. At a fundamental level, it was designed for speed, and the constraints put on it are derived from that objective.
It is reactive, which means that as soon as we are made aware of new hardware state, a handler is called. This is a significant departure from the SDK method of a loop which sequentially executes commands, but it is also the fastest option, which is why I selected it.
Additionally, it is designed to be multithreaded. Multiple handlers can (or will, the design is there but I don't have it connected to a threadpool yet) run at the same time, so that one doesn't block another. In pursuit of this goal, the only mutable state handlers are given access to is their own, and all hardware writes (there are no reads, only data requests which will be handed to a handler later) 
are non-blocking and backed by crossbeam channels (which affords immutable global state). 

The problem of needing cross-handler mutable state is solved with a global "main" thread, tasked with giving targets to the handlers, and receiving messages ("state updates") from the handlers.
Both the targets and updates are generic types, and the updates must be an enum type. State updates are effectively stored in a map, with the keys being the enum's discriminant (think: Some vs None, with what Some contains being ignored). This allows different handlers to submit state information separately, with the enum instance's contents being updated in the main thread's view of state.

Currently, there are 5 places to have code executed:
1. Gamepad handler: gamepads are considered hardware devices. Ergo, you can create handlers for them. I'm considering reworking this due to the immutable state requirements, but if you want to change targets from a gp handler, just send the target struct in a state update to be reflected back as is shown in the mecanum_with_brakes example.
2. Bulk read handler: this is currently the only robot hw read available (see roadmap) and it's pretty self-explanatory. Note: you shouldn't do heavy computation here because it's run every 1-3 ms. 
3. Packet interceptor: the basis for the Neutrino proxy, these handlers are called whenever a packet is sent to hw by the sdk. They can mutate or consume packets, send fake responses, and run in the same context as the other handlers. They are also useful for debugging.
4. Main thread: this thread is designed to block waiting for a certain state to be reached (eg. flywheel is up to speed), before sending out new targets (servo lifted). It is explicitly designed for autonomous opmodes, which is why we also have:
5. Update processors. These run before the main thread sees new state information, and can be used to react to state while the main thread is blocking. It can be used in teleops as well, to communicate Gamepad state to bulk read handlers or anything else you want to do.

### Neutrino Proxy

The "Neutrino Proxy" is an extension to BlazeFTC that is capable of drastically improving the performance of Java/Kotlin code. It does this by taking advantage of BlazeFTC's ability to proxy commands from the SDK on to the hardware while simultaneously running Rust code.

Here, it intercepts commands and immediately gives the SDK an ACK message every time it sees a motor set power command, passing on the real command to hardware. This allows your JVM code to run much faster, without having to write any Rust code. It can be found in the Quickstart, which provides instructions on its use. Because of the total divorcing of the SDK from the hardware, this actually allows proxied code to run faster than was possible with Photon (this extension's namesake) as I understand it (let me know if I'm wrong), with a basic mecanum opmode running in ~0.5 ms, and it is fully compatible with the latest versions of the FTC SDK.

It still needs to be tested against Expansion Hubs. See Roadmap. Note that Neutrino is not considered the goal of BlazeFTC, but it was useful and relatively trivial to implement so I did so anyway.

### Roadmap

BlazeFTC should be considered to be in pre-alpha. It *works* but it's missing some features. That's partially because I only have a test robot with me, and it does not have such features as an Expansion Hub, servos, or I2C devices (such as the Pinpoint). 
That's also because, at the time of writing, I literally first thought of this about 10 days ago at the time of writing this (~Dec 10th for those at home), and I haven't had time to do everything with such a tight turnaround.

Anyways, in no particular order, several things need to be implemented/tested:
+ Expansion Hub. The code is there, I'm like, 70% confident it will work. If someone could test it that'd be great.
+ Servos. These are controlled by PWM and different servos seem to want different ranges. The code is there for the default case, I have no idea of it will work or not. I'm like, 20% confident these will work so I personally wouldn't touch them. If you would like to help me implement them, that would be much appreciated.
+ I2C. The Pinpoint in particular. I have not written the code for this.
+ Pathing. For obvious reasons, JVM pathing libraries cannot be used (I mean, they could. But like... why even use this library then). BlazeFTC needs a builtin pathing library. While this is a high priority obviously, I am not the best equipped to create it and help would be appreciated in pretty much any form.
+ Configurables. Currently the only way to change runtime parameters (other than the opmode number) is by recompiling. I would like to integrate with Panels and/or acme dashboard, but for obvious reasons this is made difficult by the nature of the project. I need to be able to dynamically crate configurables at runtime, so go yell at Lazar until he adds the methods. /s
+ Support for languages other than rust. It is possible to expose a C API from Rust, and pretty much every other language can communicate with a C API. It is conceivable that we could embed/serve an API to C, MicroPython, WASM (so JS, C#, Go, etc.), Lua, and a bunch of others. I'm not sure who would find that useful, but it is within the realm of possibility.
+ There is a known bug where, apparently randomly, the FTC SDK fails to continue to communicate with the Lynx Module after BlazeFTC starts. It happens maybe 20% of the time, I expect it to be a small fix (it happens because the SDK read thread isn't locked when we take over. Probably.)
+ Graceful termination. Currently the robot restarts when you hit the stop button.
