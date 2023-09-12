#include "boxes2d.hpp"

// #define EDIT_EXTENSION

namespace rerun {
    namespace archetypes {

#ifdef EDIT_EXTENSION
        // [CODEGEN COPY TO HEADER START]

        /// Creates new `Boxes2D` with `half_sizes` centered around the local origin.
        static Boxes2D from_half_sizes(std::vector<components::HalfSizes2D> _half_sizes) {
            Boxes2D boxes;
            boxes.half_sizes = std::move(_half_sizes);
            return boxes;
        }

        /// Creates new `Boxes2D` with `centers` and `half_sizes`.
        static Boxes2D from_centers_and_half_sizes(
            std::vector<components::Origin2D> _centers,
            std::vector<components::HalfSizes2D> _half_sizes
        ) {
            return Boxes2D::from_half_sizes(std::move(_half_sizes))
                .with_centers(std::move(_centers));
        }

        /// Creates new `Boxes2D` with `half_sizes` created from (full) sizes.
        ///
        /// TODO(#3285): Does *not* preserve data as-is and instead creates half-sizes from the
        /// input data.
        static Boxes2D from_sizes(const std::vector<datatypes::Vec2D>& sizes);

        /// Creates new `Boxes2D` with `centers` and `half_sizes` created from centers and (full)
        /// sizes.
        ///
        /// TODO(#3285): Does *not* preserve data as-is and instead creates centers and half-sizes
        /// from the input data.
        static Boxes2D from_centers_and_sizes(
            std::vector<components::Origin2D> centers, const std::vector<datatypes::Vec2D>& sizes
        ) {
            return from_sizes(sizes).with_centers(std::move(centers));
        }

        /// Creates new `Boxes2D` with `half_sizes` and `centers` created from minimums and (full)
        /// sizes.
        ///
        /// TODO(#3285): Does *not* preserve data as-is and instead creates centers and half-sizes
        /// from the input data.
        static Boxes2D from_mins_and_sizes(
            const std::vector<datatypes::Vec2D>& mins, const std::vector<datatypes::Vec2D>& sizes
        );

        // [CODEGEN COPY TO HEADER END]
#endif
        Boxes2D Boxes2D::from_sizes(const std::vector<datatypes::Vec2D>& sizes) {
            std::vector<components::HalfSizes2D> half_sizes;
            half_sizes.reserve(sizes.size());
            for (const auto& wh : sizes) {
                half_sizes.emplace_back(wh.x() / 2.0, wh.y() / 2.0);
            }

            return Boxes2D::from_half_sizes(std::move(half_sizes));
        }

        Boxes2D Boxes2D::from_mins_and_sizes(
            const std::vector<datatypes::Vec2D>& mins, const std::vector<datatypes::Vec2D>& sizes
        ) {
            auto boxes = from_sizes(sizes);

            auto num_centers = std::min(mins.size(), sizes.size());
            std::vector<components::Origin2D> centers;
            centers.reserve(num_centers);
            for (size_t i = 0; i < num_centers; ++i) {
                centers.emplace_back(
                    mins[i].x() + boxes.half_sizes[i].x(),
                    mins[i].y() + boxes.half_sizes[i].y()
                );
            }

            return boxes.with_centers(centers);
        }
    } // namespace archetypes
} // namespace rerun
