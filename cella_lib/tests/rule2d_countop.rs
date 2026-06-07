use cella_lib::*;

// TODO where is this used?
fn center_apply(sub: &Rule2DSubrule, current: &CellType, crit: &CellType, points: &[(i32,i32)]) -> Option<CellType> {
    // Build a small neighbor set from a list of relative offsets
    use std::collections::HashSet;
    let set: HashSet<(i32,i32)> = points.iter().copied().collect();
    sub.applies_and_output(current, |dx, dy| {
        if set.contains(&(dx, dy)) { crit.clone() } else { CellType::inactive() }
    }).map(|ct| ct.clone())
}

#[test]
fn countop_validation_success_variants() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // eq only valid without limit
    let eq_ok = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Eq, 1, Neighborhood2D::Moore, b.clone(), None, None);
    assert!(eq_ok.validate().is_ok());

    // gt with no limit
    let gt_ok = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, None);
    assert!(gt_ok.validate().is_ok());

    // lt with no limit
    let lt_ok = Rule2DSubrule::new(a.clone(), b.clone(), 1, CountOp::Lt, 1, Neighborhood2D::Moore, b.clone(), None, None);
    assert!(lt_ok.validate().is_ok());

    // between via gt with upper inclusive limit
    let between_gt = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, Some(3));
    assert!(between_gt.validate().is_ok());

    // between via lt with lower inclusive limit
    let between_lt = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Lt, 1, Neighborhood2D::Moore, b.clone(), None, Some(2));
    assert!(between_lt.validate().is_ok());
}

#[test]
fn countop_validation_failure_variants() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    // eq with limit is invalid
    let eq_bad = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Eq, 1, Neighborhood2D::Moore, b.clone(), None, Some(3));
    assert_eq!(eq_bad.validate(), Err(RuleError::InvalidRange2D));

    // gt with limit < count invalid
    let gt_bad = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), None, Some(2));
    assert_eq!(gt_bad.validate(), Err(RuleError::InvalidRange2D));

    // lt with limit > count invalid
    let lt_bad = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Lt, 1, Neighborhood2D::Moore, b.clone(), None, Some(3));
    assert_eq!(lt_bad.validate(), Err(RuleError::InvalidRange2D));

    // range must be >=1
    let range_bad = Rule2DSubrule::new(a.clone(), b.clone(), 1, CountOp::Gt, 0, Neighborhood2D::Moore, b.clone(), None, None);
    assert_eq!(range_bad.validate(), Err(RuleError::InvalidRange2D));

    // randomness in [0,1]
    let rand_bad = Rule2DSubrule::new(a.clone(), b.clone(), 1, CountOp::Gt, 1, Neighborhood2D::Moore, b.clone(), Some(1.5), None);
    assert_eq!(rand_bad.validate(), Err(RuleError::InvalidRandomness));
}

#[test]
fn countop_applies_eq_exact() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let out = CellType::from("O");
    let sub = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Eq, 1, Neighborhood2D::Moore, out.clone(), None, None);
    // Provide exactly three neighbors
    let res = center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]);
    assert_eq!(res, Some(out.clone()));
    // Two neighbors should not pass
    let res2 = center_apply(&sub, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res2, None);
}

#[test]
fn countop_applies_gt_and_lt() {
    let a = CellType::from("A");
    let b = CellType::from("B");
    let outg = CellType::from("OG");
    let outl = CellType::from("OL");
    let gt = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, outg.clone(), None, None);
    let lt = Rule2DSubrule::new(a.clone(), b.clone(), 1, CountOp::Lt, 1, Neighborhood2D::Moore, outl.clone(), None, None);
    // gt: 2 neighbors should pass, 1 should not
    let res_gt2 = center_apply(&gt, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res_gt2, Some(outg.clone()));
    let res_gt1 = center_apply(&gt, &a, &b, &[(-1,0)]);
    assert_eq!(res_gt1, None);
    // lt: 1 neighbor should pass, 2 should not (since <=1)
    let res_lt1 = center_apply(&lt, &a, &b, &[(-1,0)]);
    assert_eq!(res_lt1, Some(outl.clone()));
    let res_lt2 = center_apply(&lt, &a, &b, &[(-1,0),(1,0)]);
    assert_eq!(res_lt2, None);
}

#[test]
fn countop_applies_between_gt() {
    // between via Gt: count..=limit
    let a = CellType::from("A");
    let b = CellType::from("B");
    let out = CellType::from("OB");
    let sub = Rule2DSubrule::new(a.clone(), b.clone(), 2, CountOp::Gt, 1, Neighborhood2D::Moore, out.clone(), None, Some(3));
    // 2 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0)]), Some(out.clone()));
    // 3 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]), Some(out.clone()));
    // 1 neighbor -> fail
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0)]), None);
    // 4 neighbors -> fail
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1),(0,-1)]), None);
}

#[test]
fn countop_applies_between_lt() {
    // between via Lt: limit..=count
    let a = CellType::from("A");
    let b = CellType::from("B");
    let out = CellType::from("OC");
    let sub = Rule2DSubrule::new(a.clone(), b.clone(), 3, CountOp::Lt, 1, Neighborhood2D::Moore, out.clone(), None, Some(2));
    // 2 neighbors -> pass (>=2 and <=3)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0)]), Some(out.clone()));
    // 3 neighbors -> pass
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1)]), Some(out.clone()));
    // 1 neighbor -> fail (below lower bound 2)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0)]), None);
    // 4 neighbors -> fail (above upper count 3)
    assert_eq!(center_apply(&sub, &a, &b, &[(-1,0),(1,0),(0,1),(0,-1)]), None);
}
