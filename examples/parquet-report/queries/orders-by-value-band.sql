select case
           when revenue < 50 then 'Under 50'
           when revenue < 100 then '50 to 99'
           when revenue < 200 then '100 to 199'
           when revenue < 400 then '200 to 399'
           when revenue < 800 then '400 to 799'
           else '800 and over'
       end as order_value,
       count(*) as orders
from orders
group by order_value
order by min(revenue);
