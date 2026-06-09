# Making FIRST Tech Challenge Code 4x Faster By Rewriting it in Rust (kinda)

Over the past few months, I have been working on a project I call "BlazeFTC," with the goal of making much better use of the hardware available to teams in the FIRST Tech Challenge. 
I've gotten a lot of questions about how it works, so in the interest of having something to point people to, this page should explain, more or less, what's going on.

In broad terms, BlazeFTC is a competition-legal robotics runtime and a rewrite of the Lynx Module communication protocol in Rust. It aims to increase the speed of FTC robot code, making both hardware reads and writes parallelizable by injecting middleware between the SDK and hardware. This allows both Java and Rust Opmodes to execute pathing algorithms at over 200 hz, compared to a typical 50 hz achievable with a stock SDK.

## The background
In 2005, the organization FIRST began to run something called the "FIRST Tech Challenge." For those who don't know, this is a High School robotics competition that thousands of teams compete in annually.
It has a smaller scope than the older, similarly named "FIRST Robotics Competition," which uses much larger robots.
Researching the history of these two competitions is left to the reader, but the basic history is important. FTC was created as a lower budget, more entry level competition and this has significantly influenced technical decisions in its software and hardware stack, and certain tradeoffs that were made.

## The Hardware

FTC was designed to be an entry level robotics competition and its hardware reflects that. In the early days, it worked as follows: FIRST produced an "Expansion Hub," a dedicated device with 4 H-bridge motor drivers, quadrature motor encoders, i2c ports, servo ports, RS485 and a few others. This board (inside an enclosure, of course) would be supplemented with a team-procured Android phone. The phone would be connected via a physical USB serial connection to the Expansion Hub.

Another device, a "Driver Hub," was also created, based on some kind of existing Android industrial thing, but it doesn't matter for the purposes of this writeup, so I will not discuss it.

This strategy of using existing hardware and software had numerous benefits. FIRST did not have to design any complicated hardware. They had to program a microcontroller, but not any kernel drivers or anything close to a multipurpose computer. Android also has extensive developer support in the form of things like ADB, and now Android Studio. Even better, Android's Java is a well-supported language many people know, and it's easier to learn for a High School robotics challenge than C (arguably). It's also worth mentioning that many people already have android phones, lowering the barrier of entry.

Later, FIRST would attempt a similar strategy, taking an existing android TV board (the "DragonBoard 410c"), modifying it slightly, and sticking it inside the Expansion Hub to create a new Control Hub. The Expansion Hub board, or Lynx Module, was connected to the Dragonboard SBC via an internal UART connection, clocked at 400k baud.

This, it must be said, worked incredibly well for FIRST, but it did have its tradeoffs, mostly in performance overhead from the UART lines and some software things.

## The Software Things
To allow teams to control the hardware they own, FIRST provided an "SDK." The SDK is a combination of some provided dependencies, bundled native libraries, and a quickstart repo allowing anyone to compile an app containing their own code, to be uploaded to their phone/Control Hub via adb and an adb connection manager from hell known as the "REV Software Hub."

The SDK provides implementations of the protocol used to communicate with the Lynx Module, I2C drivers, hardware control logic, and finally some competition specific logic around running user code, known as "Opmodes" at the correct times.

This, however, is where the problems begin. 

## The problems
As I said before, FTC is designed to be an entry level challenge. This is reflected in hardware, but it shows up in software too. The SDK has absolutely no concept of multithreading. Without modification, this means that the bottleneck is not CPU processing power or thermal throttling, it's UART or USB latency. Because teams are forced to wait for every command to come back with an Acknowledgement packet or data, the latency of the serial bus completely determines the "loop times," or how often team-written code can interact with robot hardware.

In a typical loop, team code must write to 5-8 motors, do 1-2 bulk encoder reads, and many do a 2-command i2c read to an odometry coprocessor such as the Gobuilda Pinpoint or the OTOS localizer, which gives location and velocity data. Others may do an expensive 1-3 extra i2c reads for things like color sensors, distance sensors, and IMUs. 
Each packet takes about 2 milliseconds to return after being sent, with the result being that most achieve 20-30 ms loops, with equates to 30-50 updates per second. But why do we care?

## Why we care
Controlling robots is really hard. To do this, almost all FTC teams use feedback algorithms like Proportional Integral Derivative (PID) controllers to drive their motors. The "feedback" part means that we MUST know the current robot's state to react and control it, driving our motor or drivetrain to a setpoint.
Industrial robots typically run in the kHz range with dedicated hardware for direct motion control, and even the slower processes are running at several hundred hz. Compared to our best, which is around 50 hz, there's a lot of room for improvement.  

Of course, it's important to not get carried away chasing lower loop times for no reason. Luckily, there is good evidence that loop times are holding us back in FTC. 
I ran several tests, which you can find data and notes for [here](https://www.desmos.com/calculator/hdlejttgzf), using a moderately tuned Pedropathing and basic 48 inch routes at different loop times of 12 ms, 26 ms, and 43 ms. The 43 ms test overshot by 2.3 inches, the 26 ms test by 0.77 inches, and the 12 ms test had no discernible overshoot. This is by no means a particularly scientific test, but it is representative of the situation.

Higher loop times leads to less consistent pathing. Teams may spend weeks trying to tune magic constants to perfect values which, by sheer chance, often cause the robot to come to a stop where they want it to. The issue is, jitter, battery voltage sag, obstacles, uneven floors, and random chance all combine to make this strategy incredibly inconsistent, earning team programmers everyone's annoyance when their autos fail randomly during competition. 

## Well, what do we do about this?
As I said before, the issue stopping us from reducing loop times is the fact that the SDK blocks on every packet (yes I know motor writes are weird bear with me here). I am not the first to figure this out. A library known as "Photon" was written several years ago, which can bypass the single command restriction for packets that do not require a response beyond an "Ack." This is better, but not quite good enough (also it's unmaintained now). We can do much better with a similar idea.

My solution was to figure out a way to both not block on writes and also not block on reads, as far as possible. If this can be done, it should also be possible to have multiple read packets in flight at the same time. While it would probably be possible to take advantage of this inside the SDK, I decided to go with the nuclear option. Instead of trying to force the SDK to work how we want it to, I rewrote most of the thing in Rust.

## Rewriting most of the thing in Rust
This strategy does have its disadvantages. First, far fewer FTC participants know Rust and very few people with be able to give me technical assistance in any form. Mainly though, it ended up being far more effort and time investment. However, neither of these problems actually matter, because I'm an unemployed hobbyist developer with plenty of time to work on this, and no one was too likely to write me code for this project even if it was in Java. Additionally, I don't have to spend as many days fighting with the inherent weirdness of the SDK, and it's undocumented features and TODO comments written by an unknown developer a decade ago.

Rust also has its advantages. We get total control over almost everything, native code doesn't have Garbage Collector pauses, it's faster, and also, I happen to like Rust, which can't be ignored. We are constrained to write something that is competition legal though, so I had to design the software to run inside the SDK, while still giving us all the benefits. The solution ended up being to create a kind of proxy in between the SDK and the UART bus. Anything sent/read by the SDK is intercepted by some fake java.io.Input/OutputStream and instead handed over to a compiled Rust binary via Java Native Interface. Then Blaze allows those packets to reach underlying hardware while also doing its own things, via some multithreaded shenanigans.

## The Multithreaded Shenanigans
Because I am allowed to do whatever I want inside the proxy itself, I did everything possible to maximize performance (might as well, right?). Every type of hardware interaction is built around "handlers," which will be familiar to anyone who has programmed a button to do something. At the beginning of the opmode, one or more bulk reads or i2c read packets may be dispatched. Every time we get, say, a bulk read response back, the user handler is called. It is provided handler-specific mutable state to remember things, and references to immutable global state to interact with underlying hardware. That global state is full of Atomics and Crossbeam channels to send things to the other threads.

The different handlers can be used for i2c devices, bulk reads, the gamepads (xbox controllers), and SDK packet interception, which can be used to control a lot of SDK behavior. There also has to be a way to orchestrate behavior. The handlers are intended for use as PID or drivetrain controllers, but they can't talk to each other directly. For this reason, there is a "Main Thread." Unlike the handlers, it blocks waiting for the robot to reach a certain state, as reported by the handlers. It can also send targets back to the handlers. Both state and targets are only editable by the handlers and the Main Thread, respectively, and they are backed by a combination of Crossbeam channels and HashMap<TypeId, ???> structs. Finally, the Main Thread is allowed to react to state events while blocking, for more advanced behavior. I don't defend this system, I wouldn't do it this way again due to its complexity, but it does work, and it's very fast. Ultimately, Blaze is designed to allow for speed increases in Java as well, so what the other threads are doing matters more to the average user.

```text
Main Thread
  |      ^
  v      |
Targets  State
  |      ^
  v      |
  Handlers <-- Hardware Reads
  |
  v
Write Threads
```

## What The Other Threads Are Doing
Inside Blaze, there are a couple of threads passing data around to process packets. Packets are received via byte buffers from JNI and deserialized into useful objects before being written to a dedicated Crossbeam channel (unless otherwise stated, all channels are the Crossbeam ones). Packets also come from the user's opmode/handlers via a second channel. Both channels are read from by a Write Proxy thread, which is in charge of mutating packet ids so they aren't reused, among other things. The ids are unsigned and 8-bit, so we don't have many. The Write Proxy thread then sends them via a channel to the write thread, which actually owns the File Descriptor to the UART bus.

```text
  JNI (Java) OutputStream
      |
      v
Write Proxy Thread <-- Blaze Opmode
      |
      v
UART Write Thread
```

The read system works in the opposite way. A read thread with a File Descriptor reads every packet, deserializes them, and sends them to the Read Proxy thread. It also performs some logic related to how we can only put one packet at a time on the RS485 line, but I don't want to talk about that. The read proxy thread determines (based on packet/message id) if the packet is for SDK or Blaze, and sends the packet to the correct channel. The Blaze opmode will hand the packet to the right handler.

```text
  UART Read Thread
      |
      v
Read Proxy Thread --> Blaze Opmode
      |
      v
JNI (Java) InputStream Buffer
```

This sounds complex, and it is, but it has its benefits. Everything possible was done to parallelize and pipeline everything, and the results are clear. If we take advantage of the fact that we can put reads in flight before reacting to hardware state, we can run a bulk read (4 motor PIDs) at upwards of 500 hz easily in ideal conditions. 
Code can be run even faster if multiple bulk reads are put in flight simultaneously. I2C reads, meanwhile, can be run at just over 200 hz (data [here](https://www.desmos.com/calculator/pycmnq0kbi)) with impressively low jitter (over 800 loops w/ a mean 4.7 ms loop time, the max loop time was 6.5 ms and the vast majority were much closer). Of course, this still requires that users write code in Rust. Obviously, a bridge to send data to Java is still necessary.

## Java Is Still Necessary
To allow other teams to make use of Blaze, I wrote a system to send arbitrary data back into the JVM. Historically, I've called this area of Blaze the "Neutrino Proxy" as a nod to Photon, and that's what it's called in a bunch of places still. There are several parts to this system. First, I abused Blaze's Man-In-The-Middle capability to intercept motor writes and immediately send Acks back to the SDK. This didn't have much effect, and I'm not fully sure why. Next, I wrote code to replace the DcMotorEx objects in the hardwareMap with fake ones which send motor requests directly over the JNI barrier, bypassing the Java serialization stack entirely. This seems to work a bit better. Finally, I wrote an extensible callback system. It's complex, but in effect it gives the JVM named notifications containing byte payloads. The JVM is also given the ability to set "properties" which inform i2c driver behavior in Rust. These systems are used to initialize very fast, asynchronous i2c drivers. Currently, it is possible to provide Blaze with an "on new Pinpoint data" closure, called every time data is available. Because of Blaze's multithreading, this does not decrease loop times at all. The end result of this is that anyone is fully able to run Pedropathing or Roadrunner at over 200 hz. This frees your opmode to run its normal PIDs at accelerated rates as well. This should work for anyone, so long as you don't have any issues I didn't mention.

## Issues I Didn't Mention
The main issues are hardware based. For one, I don't support OTOS or any localizers other than pinpoint. This isn't for technical reasons, it's because I don't have them and I'm lazy, so if you'd like to implement that feel free to.

Second, RS485 is still a problem. The technical issue is that RS485, the cable that connects most teams' Expansion and Control Hubs, is not bidirectional. This forces us to only write one packet to it at a time. This hurts a bit, but it's ultimately fine because drivetrains are usually on the Control Hub.

Finally, I do not support Expansion Hub over USB. This is because Android's USB stack is complicated and difficult to support, our team doesn't use it, and you can get speedups over stock SDK by switching to Blaze and RS485.
