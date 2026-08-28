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
- ✅ PedroPathing integration
- ⚠️ Exhub over USB (active WIP, working but extremely unstable)
- ⚠️ OTOS Localizer, Rev color sensor, other i2c devices not yet parallel
- ⚠️ Servo Hub - "working" but unstable

If you have questions, please check out [robotics.md](https://github.com/owenpmckenna/blaze_ftc/blob/master/robotics.md) which explains what this project is actually doing.
If you want to write Rust opmodes, the Rust quickstart can be found [here](https://github.com/owenpmckenna/BlazeFtcQuickstart), but it's not needed to get speed boosts.
Usage for Blaze in Java/Kotlin is explained below. Note that there is a normal way of using Blaze, which will require you to extend Blaze's DummyPlugOpMode, which itself extends LinearOpMode.
The lower level usage will let you extend whichever OpMode class you want, but you will need to write more code and change a few more things in your software. 
My hope is that other project maintainers (NextFTC, SolversLib, etc.) will consider integrations to make usage easier for smaller/newer/less experienced teams. I will provide assistance if you need help making the integration.

### Normal Usage
First, add `maven { url = 'https://maven.anygeneric.dev/' }` to the `repositories` block at the top of your build.dependencies.gradle.

Next, add `implementation "dev.anygeneric:blazeftc:0.1.57"` and to your dependencies. 
You will also need `implementation 'dev.anygeneric:blazeftc_pedro:0.1.57'` if you're using the Pedro integration. Note that currently I only officially support Pedro 2 as of now. 
If you are familiar with Pedro 3, Roadrunner, or any other pathing library, ping me @anygenericname and I'll get you a dependency (or help you make your own) in like 15 minutes max (it's very easy), or look at how the Pedro 2 version is implemented.

Next, add the following class to your project. An explanation of the functions used is contained within the class in comments.
```java
import com.pedropathing.follower.Follower;
import com.pedropathing.geometry.BezierLine;
import com.pedropathing.geometry.Pose;
import com.pedropathing.paths.Path;
import com.qualcomm.hardware.lynx.LynxModule;
import com.qualcomm.robotcore.eventloop.opmode.TeleOp;
import com.qualcomm.robotcore.util.ElapsedTime;

import org.firstinspires.ftc.robotcore.external.Telemetry;
import org.firstinspires.ftc.teamcode.pedroPathing.Constants;

import dev.anygeneric.blazeftc_pedro.PedroSingleDataLocalizer;

@TeleOp(name = "Example Pedro High Speed Localization")
public class ExamplePedroSpeedLocalization extends DummyPlugOpMode {
    @Override
    public void runOpModeInBlaze() {
        //this does several important things internally. You *must* call it before anything else.
        //You can pass whatever telemetry object in you want, including the split ones that go to a web dashboard.
        //However, you need to use the object it returns, and you should under no circumstances replace it with
        //`telemetry = initializeBlazeFTC(telemetry);` which replaces the OpMode's telemetry and breaks everything.
        //Alternatively, pass a no-op telemetry and ignore its output. You still need to call it though.
        Telemetry tele = initializeBlazeFTC(telemetry);
        //Normal manual cache setup.
        for (LynxModule i : hardwareMap.getAll(LynxModule.class))
            i.setBulkCachingMode(LynxModule.BulkCachingMode.MANUAL);
        //this sends motor cmds directly to blaze, skipping Java serialization completely
        //it reaches into the hwMap to replace the motors there so don't pull the motors out/init pedro before calling it
        engageMotorAcceleration();
        //we create the follower. NOTE that this uses the pinpoint java driver to set all your settings and offsets
        Follower follower = Constants.createFollower(hardwareMap);
        waitForStart();
        ElapsedTime elt = new ElapsedTime();
        //the closure you pass will be called every time we get new data.
        //you may call `setup` at any time during the opmode, but it *must* be called before runBlazeFTC(0);
        //if you call it later, it will be ignored.
        PedroSingleDataLocalizer.setup(follower, () -> {
            tele.addData("pedro loop time (ms)", elt.milliseconds());
            elt.reset();
            follower.update();
            tele.addData("x,y", follower.getPose().getX() + ", " + follower.getPose().getY());
        });
        //this is a test path. Replace it with your team's logic
        follower.followPath(new Path(new BezierLine(new Pose(0, 0), new Pose(10, 0))));
        //this turns control over to Blaze. The 0 tells blaze to use Neutrino, not a different rust opmode.
        //If you wrote other rust opmodes, you would start them instead by passing in a different number.
        //You absolutely have to call this some time after waitForStart
        runBlazeFTC(0);

        //This should be replaced with your own code. 
        ElapsedTime elt2 = new ElapsedTime();
        while (!isStopRequested()) {
            for (LynxModule i : hardwareMap.getAll(LynxModule.class))
                i.clearBulkCache();
            //it doesn't matter what you do here
            sleep(20);
            tele.addData("main loop time (ms)", elt2.milliseconds());
            elt2.reset();
            tele.update();
        }
    }
}
```

### Lower Level Usage
Lower level usage lets you extend whatever class you want, but it requires calling some functions more directly, and also there are footguns to be aware of.

The DummyPlugOpMode class below should be used as a template. Call the BlazeFTC and BlazeDummyPlug methods directly instead of the ones in the OpMode.

The most important thing, however, is that you call `BlazeDummyPlug.closeBlazeFTC()` after the opmode is done. 
To make sure it's called, do it in a `finally` block wrapping normal user code. 
Failing to call this may force you to restart the robot should you want to start a new opmode.
```java
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
+ Expansion Hub via USB. Expansion hubs over RS485 are supported, and the architecture for USB support is mostly present. Currently, you need to use an RS485 cable. This is a high priority for fixing.
+ More I2C Devices. Pinpoints are implemented and work, but they're the only ones. Implementing more is not too hard, but I haven't had the chance and I don't have anything to test against. If you have an OTOS or would like to see 3 dead wheel localization and have time to test, ping me.
+ Pathing in Rust. This is not important, but it is something I'm interested in nonetheless. You can find the repo where I tried this over [here](https://github.com/owenpmckenna/sidestep).
