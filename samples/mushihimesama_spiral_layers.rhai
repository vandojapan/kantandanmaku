// 虫姫さま風: 螺旋状の全方位弾を複数速度で分離させる。
// 特定作品の完全再現ではなく、よく見られる発射原理の学習用サンプルです。
spawn {
    ring_count = 20;
    layers = 3;
    count = ring_count * layers;

    layer = floor(i / ring_count);
    slot = i % ring_count;
    phase = wave * deg(11) + layer * deg(2);
    angle = ring(slot, ring_count) + phase;

    speed = 210 + layer * 90;
    life = 6;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
