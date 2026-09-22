// 怒首領蜂大往生風: 高速な自機狙い扇を速度差レイヤーで重ねる。
// 特定作品の完全再現ではなく、よく見られる発射原理の学習用サンプルです。
spawn {
    ways = 7;
    layers = 3;
    count = ways * layers;

    layer = floor(i / ways);
    slot = i % ways;
    // 高密度ほど狙い角の量子化を細かくする。
    base_angle = quantize(aim(), deg(2.8125) / density);
    let span = if spread_angle > 0.0 { spread_angle } else { deg(21) };
    angle = base_angle + (slot - (ways - 1) / 2) * span / (ways - 1);

    speed = 520 + layer * 120;
    life = 4;
}

motion {
    pos = polar(angle, speed * age);
    x = origin_x + pos.x;
    y = origin_y + pos.y;
}
