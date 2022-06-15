SELECT spv
FROM shader
WHERE id in (SELECT shader_id
             FROM shader_material
                      INNER JOIN material ON material.id = shader_material.material_id
             WHERE material.name = ?);