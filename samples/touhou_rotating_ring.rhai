// 東方Project風: Waveごとに位相が進む回転全方位弾。
// 特定作品の完全再現ではなく、よく見られる発射原理の学習用サンプルです。
spawn {
    ring_count = 32;
    count = ring_count;

    phase = wave * deg(9);
    angle = ring(i, count) + phase;

    speed = 250 + (wave % 3) * 30;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
