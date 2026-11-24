select dayname(day) as weekday, count(*) as orders
from orders
group by weekday, isodow(day)
order by isodow(day);
