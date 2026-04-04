use pid::Pid;

#[derive(Clone, Copy, Debug)]
pub struct MotorPIDF {
    pid: Pid<f32>,
    f: f32,
    target: f32,
    pub launched: bool
}
impl MotorPIDF {
    pub fn new(p: f32, i: f32, d: f32, f: f32) -> MotorPIDF {
        let limit = 1.0 - f;
        let mut pid = Pid::new(0.0, limit);
        if p != 0.0 {
            pid.p(p, limit);
        }
        if i != 0.0 {
            pid.i(i, limit);
        }
        if d != 0.0 {
            pid.d(d, limit);
        }
        MotorPIDF {pid, f, target: 0.0, launched: false}
    }
    pub fn maybe_update_pids(&mut self, pids: &PIDF) -> bool {
        let mut updated = false;
        let limit = 1.0 - pids.3;
        if self.pid.kp != pids.0 {
            self.pid.p(pids.0, limit);
            updated = true;
        }
        if self.pid.ki != pids.1 {
            self.pid.i(pids.1, limit);
            updated = true;
        }
        if self.pid.kd != pids.2 {
            self.pid.d(pids.2, limit);
            updated = true;
        }
        if self.f != pids.3 {
            self.f = pids.3;
            updated = true;
        }
        if updated {
            self.pid.reset_integral_term()
        }
        updated
    }
    pub fn set_target(&mut self, target: f32) {
        self.launched = true;
        if self.target == target {
            return;
        }
        self.target = target;
        self.pid.setpoint(target);
        self.pid.reset_integral_term();
    }
    pub fn update(&mut self, new_data: f32) -> f32 {
        if !self.launched {
            return 0.0
        }
        let mut out = self.pid.next_control_output(new_data).output;
        if out > 0.0 {
            out += self.f;
        } else if out < 0.0 {
            out -= self.f;
        }
        out.min(1.0).max(-1.0)
    }
    pub fn get_target(&self) -> f32 {
        self.target
    }
    pub fn reset_integral(&mut self) { self.pid.reset_integral_term() }
}
pub type PIDF = (f32, f32, f32, f32);