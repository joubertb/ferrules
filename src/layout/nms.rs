pub fn multiclass_nms_class_agnostic(
    boxes: &Array<f32, Dim<[usize; 2]>>,
    scores: &Array<f32, Dim<[usize; 2]>>,
    nms_thr: f32,
    score_thr: f32,
) -> Array2<f32> {
    let cls_inds = Array1::from_iter(scores.axis_iter(Axis(0)).map(|e| {
        let (max_i, _max) = e.iter().enumerate().fold((0_usize, 0_f32), |acc, (i, e)| {
            let (max_i, max) = acc;
            if *e > max {
                (i, *e)
            } else {
                (max_i, max)
            }
        });
        max_i
    }));

    let cls_scores = Array1::from_iter(
        scores
            .axis_iter(Axis(0))
            .zip_eq(cls_inds.iter())
            .map(|(e, i)| e[*i]),
    );

    let valid_score_mask = cls_scores.mapv(|s| s > score_thr);
    let valid_scores = Array1::from_iter(
        cls_scores
            .iter()
            .zip_eq(valid_score_mask.iter())
            .filter(|(_, b)| **b)
            .map(|(s, _)| *s),
    );

    let valid_boxes: Array2<f32> = to_array2(
        &boxes
            .outer_iter()
            .zip_eq(valid_score_mask.iter())
            .filter(|(_, b)| **b)
            .map(|(s, _)| s.to_owned())
            .collect::<Vec<_>>(),
    )
    .unwrap();

    let valid_cls_inds = Array1::from_iter(
        cls_inds
            .iter()
            .zip_eq(valid_score_mask.iter())
            .filter(|(_, b)| **b)
            .map(|(s, _)| s)
            .collect::<Vec<_>>(),
    );

    let keep = nms(&valid_boxes.to_owned(), &valid_scores, nms_thr);

    let valid_boxes_vec: Vec<_> = valid_boxes.outer_iter().collect();
    let valid_boxes_kept = to_array2(
        &keep
            .iter()
            .map(|i| valid_boxes_vec[*i])
            .map(|e| e.to_owned())
            .collect::<Vec<_>>(),
    )
    .unwrap();

    let valid_scores_vec: Vec<_> = valid_scores.into_iter().collect();
    let valid_scores_kept = to_array2(
        &keep
            .iter()
            .map(|i| valid_scores_vec[*i])
            .map(|e| Array1::from_elem(1, e))
            .collect::<Vec<_>>(),
    )
    .unwrap();

    let valid_cls_inds_vec: Vec<_> = valid_cls_inds.into_iter().collect();
    let valid_cls_inds_kept = to_array2(
        &keep
            .iter()
            .map(|i| valid_cls_inds_vec[*i])
            .map(|e| Array1::from_elem(1, e))
            .collect::<Vec<_>>(),
    )
    .unwrap();

    let dets = concatenate(
        Axis(1),
        &[
            valid_boxes_kept.view(),
            valid_scores_kept.view(),
            valid_cls_inds_kept.mapv(|e| *e as f32).view(),
        ],
    )
    .unwrap();

    return dets;
}

fn nms(
    boxes: &Array<f32, Dim<[usize; 2]>>,
    scores: &Array<f32, Dim<[usize; 1]>>,
    nms_thr: f32,
) -> Vec<usize> {
    let x1 = boxes.slice(s![.., 0]);
    let y1 = boxes.slice(s![.., 1]);
    let x2 = boxes.slice(s![.., 2]);
    let y2 = boxes.slice(s![.., 3]);

    let areas = (&x2 - &x1 + 1_f32) * (&y2 - &y1 + 1_f32);
    let mut order = {
        let mut o = utils::argsort_by(&scores, |a, b| a.partial_cmp(b).unwrap());
        o.reverse();
        o
    };

    let mut keep = vec![];

    while !order.is_empty() {
        let i = order[0];
        keep.push(i);

        let order_sliced = Array1::from_iter(order.iter().skip(1));

        let xx1 = order_sliced.mapv(|o_i| f32::max(x1[i], x1[*o_i]));
        let yy1 = order_sliced.mapv(|o_i| f32::max(y1[i], y1[*o_i]));
        let xx2 = order_sliced.mapv(|o_i| f32::min(x2[i], x2[*o_i]));
        let yy2 = order_sliced.mapv(|o_i| f32::min(y2[i], y2[*o_i]));

        let w = ((&xx2 - &xx1) + 1_f32).mapv(|v| f32::max(0.0, v));
        let h = ((&yy2 - &yy1) + 1_f32).mapv(|v| f32::max(0.0, v));
        let inter = w * h;
        let ovr = &inter / (areas[i] + order_sliced.mapv(|e| areas[*e]) - &inter);

        let inds = Array1::from_iter(
            ovr.iter()
                .map(|e| *e <= nms_thr)
                .enumerate()
                .filter(|(_, p)| *p)
                .map(|(i, _)| i),
        );

        drop(order_sliced);

        order = inds.into_iter().map(|i| order[i + 1]).collect();
    }

    return keep;
}
