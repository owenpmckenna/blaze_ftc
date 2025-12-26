use pid::Pid;
#[derive(Clone, Copy, Debug)]
pub struct MotorPIDF {
    pid: Pid<f32>,
    f: f32,
    target: f32
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
        MotorPIDF {pid, f, target: 0.0}
    }
    pub fn set_target(&mut self, target: f32) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.pid.setpoint(target);
    }
    pub fn update(&mut self, new_data: f32) -> f32 {
        let mut out = self.pid.next_control_output(new_data).output;
        if out > 0.0 {
            out += self.f;
        } else if out < 0.0 {
            out += self.f;
        }
        out
    }
    pub fn get_target(&self) -> f32 {
        self.target
    }
}