// 怒首領蜂大往生風: 高速な自機狙い扇を速度差レイヤーで重ねる。
// 特定作品の完全再現ではなく、よく見られる発射原理の学習用サンプルです。
spawn {
    ways = 7;
    layers = 3;
    count = ways * layers;

    layer = floor(i / ways);
    slot = i % ways;
    base_angle = quantize(aim(), deg(2.8125));
    angle = base_angle + (slot - (ways - 1) / 2) * deg(3.5);

    speed = 520 + layer * 120;
    life = 4;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
