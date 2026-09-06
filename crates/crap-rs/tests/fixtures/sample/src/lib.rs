pub fn trivial() {
    let _x = 1;
}

pub fn moderate(x: i32) -> i32 {
    if x > 0 && x < 10 { x } else { 0 }
}

pub fn crappy(x: i32) -> i32 {
    if x == 0 {
        return 0;
    }
    if x == 1 {
        return 1;
    }
    if x == 2 {
        return 2;
    }
    if x == 3 {
        return 3;
    }
    if x == 4 {
        return 4;
    }
    if x == 5 {
        return 5;
    }
    if x == 6 {
        return 6;
    }
    if x == 7 {
        return 7;
    }
    if x == 8 {
        return 8;
    }
    if x == 9 {
        return 9;
    }
    x
}

pub fn question() -> Result<(), ()> {
    Ok(())?;
    Ok(())
}
