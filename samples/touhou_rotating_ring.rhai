// 東方Project風: Waveごとに位相が進む回転全方位弾。
// 特定作品の完全再現ではなく、よく見られる発射原理の学習用サンプルです。
spawn {
    ring_count = 32;
    count = ring_count;

    // 密度でWave数が変わっても、1秒あたりの位相回転量は保つ。
    phase = wave * deg(9) / density;
    let span = if spread_angle > 0.0 { spread_angle } else { deg(360) };
    let divisor = if span < deg(360) { count - 1 } else { count };
    angle = i * span / divisor + phase;

    speed = 250 + (wave % 3) * 30;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
