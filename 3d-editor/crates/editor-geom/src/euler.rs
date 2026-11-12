use glam::{DMat3, DVec3};

/// Orientation for `[pitch, yaw, roll]` degrees: `Rz(yaw) * Ry(-pitch) * Rx(roll)`.
pub fn orientation(angles: DVec3) -> DMat3 {
    DMat3::from_rotation_z(angles.y.to_radians())
        * DMat3::from_rotation_y(-angles.x.to_radians())
        * DMat3::from_rotation_x(angles.z.to_radians())
}

/// Inverse of `orientation`, rounded to 1e-6 degrees.
pub fn angles_of(m: DMat3) -> DVec3 {
    let f = m.x_axis;
    let yaw = f.y.atan2(f.x);
    let pitch = f.z.clamp(-1.0, 1.0).asin();
    let base = DMat3::from_rotation_z(yaw) * DMat3::from_rotation_y(-pitch);
    let r = base.transpose() * m;
    let roll = r.y_axis.z.atan2(r.y_axis.y);
    DVec3::new(pitch, yaw, roll).map(|a| {
        let degrees = a.to_degrees();
        (degrees * 1e6).round() / 1e6 + 0.0
    })
}
