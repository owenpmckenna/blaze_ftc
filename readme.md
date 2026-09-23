# BlazeFTC

BlazeFTC is a (partial) Rust rewrite of the SDK of the FIRST Tech Challenge, a high school robotics competition.
Designed for speed, it offers the fastest possible loop times of any public project, thanks to it's direct hardware control, faster programming language, and "reactive" design.
It is capable of running in the context of a regular opmode, and it maintains compatibility with the FTC SDK, which can still issue commands while the BlazeFTC opmode is running.
As of now, it should be considered to be in beta. The code is there, but there's some bugs and design choices I would like input on, among other things. Actionable suggestions/pull requests are appreciated.

- ✅ Competition Legal
- ✅ Runs alongside SDK
- ✅ Code in Java/Kotlin (Rust not required, see example below)
- ✅ Parallel/nonblocking Writes (this is what Photon does)
- ✅ Parallel Reads (unique to blaze) - Pinpoint and Bulk Reads
- ✅ Exhub over RS485
- ✅ PedroPathing 2/3 integration
- ✅ Exhub over USB (working but ping me if you have issues)
- ⚠️ OctoQuad integration in progress. Ping me if you want to test out the beta version.
- ⚠️ OTOS Localizer, Rev color sensor, other i2c devices not yet parallel
- ⚠️ Servo Hub - working but unstable

If you have questions, please check out [robotics.md](https://github.com/owenpmckenna/blaze_ftc/blob/master/robotics.md) which explains what this project is actually doing.
If you want to write Rust opmodes, the Rust quickstart can be found [here](https://github.com/owenpmckenna/BlazeFtcQuickstart), but it's not needed to get speed boosts.
Usage for Blaze in Java/Kotlin is explained below. Note that there is a normal way of using Blaze, which will require you to extend Blaze's DummyPlugOpMode, which itself extends LinearOpMode.
The lower level usage will let you extend whichever OpMode class you want, but you will need to write more code and change a few more things in your software. 
My hope is that other project maintainers (NextFTC, SolversLib, etc.) will consider integrations to make usage easier for smaller/newer/less experienced teams. I will provide assistance if you need help making the integration.

### Should you use Blaze?
FTC teams have been building and programming robots for years without Blaze. If you don't use it, you will be fine. 
*However*, loop times do affect robot performance, and Blaze is capable of significantly improving loop times on many different hardware setups:
- Control Hub + Exhub over RS485 w/ Pinpoint - Blaze makes communication with Control Hub still possible while waiting on RS485. You should be able to run your drivetrain loops at 200+ hz despite the RS485 line. Exhub loops will also be faster, they don't have to wait on drivetrain. This assumes your drivetrain is on the Control Hub.
- Control Hub + Exhub over RS485 w/ 3-wheel odom - Blaze can run bulk reads at upwards of 1 kHz. RS485 cannot be improved by much but if you're using bulk reads for localization, Blaze will make your software much faster.
- Control Hub + Exhub over USB w/ Pinpoint - Run flywheel, auxiliary motor PIDs at 800-900 Hz (usb is slower), with pinpoint-dependent operations between 200 and 250 hz.  
- Control Hub + Exhub over USB w/ 3-wheel odom - Run flywheel, auxiliary motor PIDs at 800-900 Hz, and get odometry at almost 1 kHz.

Note that these numbers are based on tests in "lab conditions" as I haven't had the hardware to put Blaze through its paces fully. 
If you can confirm or refute any of this data in practical conditions let me know. I will update this list as I collect more tests.

### Normal Usage
First, add `maven { url = 'https://maven.anygeneric.dev/' }` to the `repositories` block at the top of your build.dependencies.gradle.

Next, add `implementation "dev.anygeneric:blazeftc:0.1.62"` and to your dependencies. 
You will also need `implementation 'dev.anygeneric:blazeftc_pedro:0.1.62'` if you're using the Pedro 2 integration. `implementation 'dev.anygeneric:blazeftc_pedro3:0.1.62'` has Pedro 3 integration.
If you are familiar with Roadrunner or any other pathing library, ping me @anygenericname and I'll get you a dependency (or help you make your own) in like 15 minutes max (it's very easy), or look at how the Pedro 2 version is implemented.

Next, add one of the following classes to your project. The DummyPlugOpMode class extends LinearOpMode, or if you'd rather use OpMode use the second example.
```java
@TeleOp(name = "Example Pedro High Speed Localization")
public class ExamplePedroSpeedLocalization extends DummyPlugOpMode {
    @Override
    public void runOpModeInBlaze() {
        initializeBlazeFTC();
        engageMotorAcceleration();
        //we create the pedro2 follower. NOTE that this uses the pinpoint java driver to set all your settings and offsets
        Follower follower = Constants.createFollower(hardwareMap);
        waitForStart();
        ElapsedTime elt = new ElapsedTime();
        PedroSingleDataLocalizer.setup(follower, () -> {
            telemetry.addData("pedro loop time (ms)", elt.milliseconds());
            elt.reset();
            follower.update();
            telemetry.addData("x,y", follower.getPose().getX() + ", " + follower.getPose().getY());
        });
        //this is a test path. Replace it with your team's logic
        follower.followPath(new Path(new BezierLine(new Pose(0, 0), new Pose(10, 0))));
        runBlazeFTC(0);

        //This should be replaced with your own code. 
        ElapsedTime elt2 = new ElapsedTime();
        while (!isStopRequested()) {
            for (LynxModule i : hardwareMap.getAll(LynxModule.class))
                i.clearBulkCache();
            //it doesn't matter what you do here
            sleep(20);
            telemetry.addData("main loop time (ms)", elt2.milliseconds());
            elt2.reset();
            telemetry.update();
        }
    }
}
```

OpMode version:

```java
public class BlazeOpMode extends OpMode {
    boolean motorInPlace = false;
    int target = 500;
    Follower follower = null;
    Path pathToFollow;
    @Override
    public void init() {
        BlazeDummyPlug.initializeBlazeFTC(hardwareMap);
        BlazeDummyPlug.engageMotorAccel(hardwareMap);
        follower = Constants.create(hardwareMap);
        DcMotor motor = hardwareMap.get(DcMotor.class, "motor");
        //do whatever else init stuff you need to here
        ElapsedTime elt = new ElapsedTime(ElapsedTime.Resolution.MILLISECONDS);
        Pedro3SingleDataLocalizer.setup(follower, () -> {
            telemetry.addData("pedro loop time (ms)", elt.milliseconds());
            elt.reset();
            telemetry.addData(
                    "x,y",
                    follower.pose().x() + ", " + follower.pose().y()
            );
            if (follower.currentPath() != pathToFollow) {
                follower.follow(pathToFollow);
            }
            follower.update();
        });

        ElapsedTime elt2 = new ElapsedTime(ElapsedTime.Resolution.MILLISECONDS);
        BlazeDummyPlug.engageBulkReadAcceleration(hardwareMap, Hub.ExHub, 1, () -> {
            //Every time this function is called, you should have new encoder data available in your motors. Run PID loops here.
            //I recommend doing it like this, report your data do not do computation here:
            if (motor.getCurrentPosition() == target) {
                motorInPlace = true;
            }
            telemetry.addData("bulk loop time (ms)", elt2.milliseconds());
            elt2.reset();
            return null;
        });
    }

    @Override
    public void start() {
        BlazeFTC.run(0);
    }

    @Override
    public void loop() {
        telemetry.update();
        //do what you like here. Personally I'd use a command library. but as a trivial example:
        if (motorInPlace) {
            pathToFollow = line(new Pose(0.0, 0.0), new Pose(10.0, 10.0));
            if (follower.currentPath() != null && follower.atParametricEnd()) {
                telemetry.addData("done", true);
            }
        }
    }

    @Override
    public void stop() {
        BlazeDummyPlug.closeBlazeFTC();
    }
}
```

### What do these methods do?
1. `initializeBlazeFTC()` does a few things. It loads the native Blaze code if needed, and takes over hardware control. It must always be called first. If you pass a Telemetry to it, it returns a Telemetry you should use. The old telemetry will be managed by Blaze so any Rust code can drivetrain telemetry. You should not use this feature, do not pass a telemetry.
2. `engageMotorAcceleration()` replaces the Motors in the hardwareMap with ones that feed back to Blaze instead, skipping the sdk hardware stack. Call it before calling Constants.createFollower, hardwareMap.get(), or similar. If you're not using SDK motors, see below. (Optional)
3. `PedroSingleDataLocalizer` (and its counterpart, `Pedro3SingleDataLocalizer`) isn't too complicated. Once (not before) you call `runBlazeFTC(0)`, that closure will be called every couple milliseconds when Blaze has new data for you. follower.update() is already called and shouldn't be called anywhere else. Thread safety is up in the air, so my official suggestion is to not call anything that changes the follower from outside the closure. Reads are probably fine, but I'd avoid, say, accidentally changing the target path while drive powers are actively being calculated. (Optional)
4. `engageBulkReadAcceleration(hub, packets, {})` is similar. Pass the hub you want, obviously. "Packets" is the number of read packets sent at first, and then we just send another every time we get a response. Every packet takes about 2 ms to come back, so you get 500 hz from packets = 1, and almost 1000 hz from packets = 2. Don't go higher than that. When the closure is called the data should be in your motors, so you write code like normal. (Optional)
5. `runBlazeFTC(0)` orders the native side to actually begin running. Only call it after `waitForStart()` (or equivalent). The 0 specifies that you're running the Neutrino OpMode in Blaze, if you wrote your own Rust code you might pass a different number.
6. `closeBlazeFTC()` tells Blaze to stop. It must be called or the robot will restart (will fix this eventually). The DummyPlugOpMode actually exists entirely to wrap your code in a try-finally block to ensure it gets called.

### Lower Level Functions
* `BlazeFTC.setMotorPower(int hubId, int port, double power)` does what it says on the tin. It sets the motor power but bypasses the SDK stack. HubId is literally the lynx id: `LynxModule.getModuleAddress()`. Call this from your motor implementation if you have one different from the SDK version. (If you have different motors, do not call `engageMotorAcceleration()`)
* `BlazeDummyPlug.engagePinpointAcceleration(driver, function)` is what the `PedroSingleDataLocalizer`s call. You can use it if you want more control.
* `BlazeFTC.setMotorPowers(int hub, double m0, double m1, double m2, double m3)` was an attempt as making drivetrains faster. Under the hood it does all motor writes with only one syscall. You probably shouldn't use it, it's fairly untested. If you do test it let me know. You can pass NaN to not write for a motor.

### Lower Level Usage
Lower level usage lets you extend whatever class you want, but it requires calling some functions more directly, and also there are footguns to be aware of.

The DummyPlugOpMode class below should be used as a template. Call the BlazeFTC and BlazeDummyPlug methods directly instead of the ones in the OpMode.

The most important thing, however, is that you call `BlazeDummyPlug.closeBlazeFTC()` after the opmode is done. 
To make sure it's called, do it in a `finally` block wrapping normal user code. 
Failing to call this may force you to restart the robot should you want to start a new opmode.
```kotlin
abstract class DummyPlugOpMode : LinearOpMode() {
    fun sendPropertyToRust(key: String, value: String) {
        BlazeFTC.sendProperty(key, value);
    }
    fun engageMotorAcceleration() {
        BlazeDummyPlug.engageMotorAccel(hardwareMap)
    }
    fun engagePinpointAcceleration(ppd: GoBildaPinpointDriver, acceptor: (PositionData) -> Unit) {
        BlazeDummyPlug.engagePinpointAcceleration(ppd, acceptor)
    }
    fun engageBulkReadAcceleration(ctrlHub: Hub, numberPackets: Int = 1, acceptor: () -> Unit) {
        BlazeDummyPlug.engageBulkReadAcceleration(hardwareMap, ctrlHub, numberPackets, acceptor)
    }
    @Deprecated(message = "Not \"deprecated\" per se but should not be called")
    fun engageBulkReadAccelerationAtFrequency(ctrlHub: Hub, frequency: Int, acceptor: () -> Unit) {
        BlazeDummyPlug.engageBulkReadAccelerationAtFrequency(hardwareMap, ctrlHub, frequency, acceptor)
    }
    /*this one has no telemetry, so use if my telemetry was causing problems*/
    final fun initializeBlazeFTC() {
        BlazeDummyPlug.initializeBlazeFTC(hardwareMap)
    }
    final fun initializeBlazeFTC(userTelemetry: Telemetry) : Telemetry =
        BlazeDummyPlug.initializeBlazeFTC(telemetry, hardwareMap)
    fun runBlazeFTC(toRun: Int) {
        BlazeFTC.run(toRun)
    }
    fun updateGamepads() {
        BlazeFTC.gamepad(gamepad1.toByteArray(), gamepad2!!.toByteArray())
    }
    abstract fun runOpModeInBlaze();
    override fun runOpMode() {
        try {
            runOpModeInBlaze()
        } catch (e: Throwable) {
            println("BlazeFTC's Dummy Plug OpMode caught $e")
            e.printStackTrace()
            throw e
        } finally {
            println("Closing BlazeFTC")
            BlazeDummyPlug.closeBlazeFTC()
        }
    }
}
```

### Roadmap

BlazeFTC should be considered to be in beta. It works but it's missing some features. Most things Neutrino depends on are fine but actual usability in BlazeFTC isn't quite there (eg. no pathing library exists yet, and the ergonomics aren't great all around but I'm working on it).

Anyway, in no particular order, several things need to be implemented/tested:
+ Better scheduling.
+ More I2C Devices. Pinpoints are implemented and work, but they're the only ones. Implementing more is not too hard, but I haven't had the chance and I don't have anything to test against. If you have an OTOS or would like to see 3 dead wheel localization and have time to test, ping me.
+ Pathing in Rust. This is not important, but it is something I'm interested in nonetheless. You can find the repo where I tried this over [here](https://github.com/owenpmckenna/sidestep).
