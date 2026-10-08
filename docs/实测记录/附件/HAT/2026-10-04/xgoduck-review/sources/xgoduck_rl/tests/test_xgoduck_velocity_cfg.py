"""XgoDuck walk-task DR / pose-weight overrides; MicroDuck stays on the old recipe."""

from mjlab_microduck.tasks import _xgoduck_velocity_cfg
from mjlab_microduck.tasks.microduck_velocity_env_cfg import (
    make_microduck_velocity_env_cfg,
)


def test_xgoduck_velocity_friction_push_noise_and_pose_weight():
    cfg = _xgoduck_velocity_cfg()
    actor = cfg.observations["actor"].terms

    assert cfg.events["foot_friction"].params["ranges"] == (0.4, 1.5)

    vr = cfg.events["push_robot"].params["velocity_range"]
    assert vr["x"] == (-0.2, 0.2)
    assert vr["y"] == (-0.2, 0.2)

    assert actor["base_ang_vel"].noise.n_min == -0.06
    assert actor["base_ang_vel"].noise.n_max == 0.06
    assert actor["projected_gravity"].noise.n_min == -0.03
    assert actor["projected_gravity"].noise.n_max == 0.03
    assert actor["joint_pos"].noise.n_min == -0.01
    assert actor["joint_pos"].noise.n_max == 0.01

    assert cfg.rewards["pose"].weight == 1.3
    assert cfg.rewards["pose"].params["std_standing"][r".*hip_yaw.*"] == 0.05
    assert cfg.rewards["pose"].params["std_walking"][r".*hip_yaw.*"] == 0.15
    assert cfg.rewards["pose"].params["std_running"][r".*hip_yaw.*"] == 0.25


def test_xgoduck_vin_min_is_7():
    from mjlab_microduck.robot.microduck_constants import actuators as md
    from mjlab_microduck.robot.xgoduck_constants import actuators as xg

    assert xg.vin_min == 7.0
    assert md.vin_min == 6.0


def test_microduck_velocity_keeps_original_walk_recipe():
    cfg = make_microduck_velocity_env_cfg()
    actor = cfg.observations["actor"].terms

    assert cfg.events["foot_friction"].params["ranges"] == (0.7, 1.3)
    vr = cfg.events["push_robot"].params["velocity_range"]
    assert vr["x"] == (-0.3, 0.3)
    assert actor["base_ang_vel"].noise.n_max == 0.03
    assert actor["projected_gravity"].noise.n_max == 0.01
    assert actor["joint_pos"].noise.n_max == 0.001
    assert cfg.rewards["pose"].weight == 1.0
    assert cfg.rewards["pose"].params["std_walking"][r".*hip_yaw.*"] == 0.3
